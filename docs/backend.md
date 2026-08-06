# openGuandan 后端架构与接口

## 1. 设计边界

后端是唯一可信的游戏状态持有者。客户端只提交操作意图，后端负责洗牌、发牌、牌型识别、牌型比较、出牌顺序、借风、出完顺序、升级、贡还牌和过 A 判定。

所有活动状态保存在进程内存中：

- 每轮牌进行时保存四家手牌、当前牌权、已过牌人数和贡还牌状态。
- 每轮牌结束后立即丢弃手牌和出牌过程，只保留双方级数与上一轮出完结果。
- 一局牌结束后丢弃整局状态，房间回到等待状态。
- 不保存用户名、战绩、牌谱或房间历史。
- 服务进程重启后，所有活动房间都会消失。

代码内部使用 `RoundState` 表示一轮牌，使用 `MatchState` 表示从 2 打到 A 的完整一局牌。

## 2. 目录

```text
apps/server/
  src/
    domain/       牌张、牌型、轮牌和一局牌规则
    rooms/        房间、临时身份、内存存储和命令处理
    transport/    HTTP、Socket.IO 和输入校验
    views/        玩家与旁观者的个性化状态视图
    app.ts        应用组装
    index.ts      进程入口
  test/           自动化测试
```

## 3. 环境变量

| 名称 | 默认值 | 含义 |
| --- | --- | --- |
| `HOST` | `0.0.0.0` | 监听地址 |
| `PORT` | `3004` | HTTP 与 WebSocket 端口 |
| `CORS_ORIGIN` | `http://localhost:5174` | 允许的前端源；多个源用逗号分隔 |
| `ROOM_IDLE_TTL_MS` | `600000` | 全员离线后房间的最长空闲时间 |
| `RECONNECT_GRACE_MS` | `90000` | 等待大厅中为断线参与者保留身份的时间 |

## 4. HTTP 接口

### 4.1 创建房间

```http
POST /api/rooms
Content-Type: application/json

{
  "displayName": "Alice"
}
```

返回 `201`：

```json
{
  "roomCode": "8JRMKQ",
  "participantId": "72d401af-53e9-421a-ab48-20619bb8fbe2",
  "reconnectToken": "仅发送给该浏览器的随机令牌",
  "role": "player",
  "seat": 0
}
```

创建者成为房主和 0 号座位玩家。

### 4.2 加入房间

```http
POST /api/rooms/:roomCode/join
Content-Type: application/json

{
  "displayName": "Bob"
}
```

等待阶段且尚有空座时返回 `role: "player"`；座位已满或一局牌已经开始时返回 `role: "spectator"`。同一房间内的临时用户名不能重复。

### 4.3 查询公开房间状态

```http
GET /api/rooms/:roomCode
```

该接口永远不返回任何玩家手牌或重连令牌。

### 4.4 健康检查

```http
GET /health
```

## 5. 建立 Socket.IO 连接

创建或加入房间后，前端应保存返回的三个身份字段，并将其用于连接：

```ts
import { io } from "socket.io-client";

const socket = io("http://localhost:3004", {
  auth: {
    roomCode,
    participantId,
    reconnectToken,
  },
});
```

临时用户名只用于显示。实际身份由不可猜测的 `participantId` 和 `reconnectToken` 共同确认。刷新页面后使用原凭证重新连接，即可恢复原座位和手牌。

连接成功或收到 `STALE_STATE` 后，客户端可以发送只读事件 `room.sync`。该事件不携带 `actionId` 或 `version`，服务器通过 acknowledgement 返回当前参与者的个性化 `snapshot`：

```ts
socket.emit("room.sync", (response) => {
  if (response.ok) render(response.snapshot);
});
```

## 6. 客户端命令

所有会改变状态的消息都必须包含：

```json
{
  "actionId": "客户端生成且不重复的操作 ID",
  "version": 12
}
```

- `actionId` 用于防止网络重试造成重复出牌。
- `version` 必须等于最近一次 `room.snapshot` 中的版本。
- 状态过期时服务器返回 `STALE_STATE`，客户端应等待或请求最新快照。

服务器通过 Socket.IO acknowledgement 返回：

```json
{
  "ok": true,
  "version": 13,
  "duplicate": false
}
```

失败时返回：

```json
{
  "ok": false,
  "error": {
    "code": "NOT_YOUR_TURN",
    "message": "尚未轮到该玩家行动",
    "details": null
  }
}
```

### 6.1 房间与一局牌

| 事件 | 附加字段 | 权限 |
| --- | --- | --- |
| `room.ready` | `ready: boolean` | 座位玩家 |
| `match.start` | 无 | 房主；四人已连接且全部准备 |
| `match.abort` | 无 | 房主 |
| `round.next` | 无 | 房主；上一轮已经结算 |

### 6.2 出牌

```json
{
  "actionId": "7d6fb018-1",
  "version": 13,
  "cardIds": ["0:heart:7", "0:spade:8", "1:spade:8", "0:club:8"],
  "declaration": {
    "kind": "bomb",
    "primaryRank": "8"
  }
}
```

事件名为 `round.play`。当所选牌张因为逢人配等原因可以解释为多个牌型时，必须提供 `declaration`。后端会重新验证声明，不会信任客户端判断。

可用牌型标识：

```text
single
pair
triple
full-house
straight
consecutive-pairs
consecutive-triples
bomb
straight-flush
joker-bomb
```

过牌使用 `round.pass`，只需发送通用的 `actionId` 和 `version`。

### 6.3 贡还牌

| 事件 | 附加字段 |
| --- | --- |
| `tribute.give` | `cardId` |
| `tribute.return` | `cardId` |

后端负责验证贡牌是否为最大合资格牌、红桃级牌是否被错误用于进贡，以及还牌是否不大于 10。双贡的两张牌相同时，后端按从上一轮上游座位开始的顺时针顺序确定对应关系。

## 7. 服务端事件

| 事件 | 用途 |
| --- | --- |
| `room.snapshot` | 针对当前连接生成的完整最新视图 |
| `participant.connection` | 参与者连接状态变化 |
| `room.ready` | 准备状态变化 |
| `match.started` | 一局牌开始 |
| `match.aborted` | 房主终止当前一局牌 |
| `round.started` | 新一轮牌开始 |
| `round.finished` | 出完顺序与升级结果 |
| `tribute.completed` | 公开最终贡牌、还牌和首出座位 |
| `match.finished` | 一方成功过 A |

`room.snapshot` 是状态同步的最终依据；其他事件用于动画、提示和结算界面。

## 8. 隐藏信息与旁观

服务器为每个连接单独生成状态视图：

- 座位玩家在 `self.hand` 中收到自己的 27 张牌。
- 旁观者的 `self.hand` 始终为空。
- 所有人都能看到座位、用户名、剩余牌数、当前公开出牌、当前行动座位、级数和出完顺序。
- 任何视图都不包含其他玩家的手牌或身份令牌。
- 游戏开始后新加入的参与者固定为旁观者，不会接管断线玩家的座位。

## 9. 断线和清理

- 座位玩家断线后，一局牌暂停，直到其使用原重连令牌回来。
- 房主可以使用 `match.abort` 结束无法继续的一局牌。
- 等待阶段的断线参与者超过宽限时间后会被移出房间并释放座位。
- 所有人离线且房间超过空闲期限后，房间从内存中删除。
- 当前实现是单进程部署；若以后需要多实例，应将活动房间状态和 Socket.IO adapter 迁移到带 TTL 的 Redis，但仍无需保存历史牌谱。
