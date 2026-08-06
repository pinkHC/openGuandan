# openGuandan

掼蛋网站单仓库。目前已经实现服务端权威后端，并为前端应用预留独立目录。

```text
apps/
  server/   TypeScript、Fastify、Socket.IO 后端
  web/      前端应用目录
docs/       规则、接口和设计文档
```

## 运行

要求 Node.js 20 或更高版本。

```powershell
npm install
Copy-Item apps/server/.env.example apps/server/.env
npm run dev:server
```

默认监听 `http://localhost:3000`。健康检查：

```powershell
Invoke-RestMethod http://localhost:3000/health
```

生产构建：

```powershell
npm run build
npm run start:server
```

验证：

```powershell
npm test
npm run typecheck
```

## 文档

- [掼蛋规则](./docs/rules.md)
- [后端架构与接口](./docs/backend.md)
- [牌型中英文名称](./docs/card-types.json)
