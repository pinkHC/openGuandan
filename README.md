# openGuandan

掼蛋网站单仓库，包含服务端权威后端和响应式实时对战前端。

```text
apps/
  server/   Rust、Axum、socketioxide 后端
  web/      React、Vinext 前端
docs/       规则、接口和设计文档
```

## 运行

要求 Node.js 22.13 或更高版本，以及 Rust 1.94 或更高版本。Rust 工具链需要包含
`rustfmt` 和 `clippy` 组件。

```powershell
npm install
Copy-Item apps/server/.env.example apps/server/.env
npm run dev
```

前端默认运行在 `http://localhost:5174`，后端默认运行在 `http://localhost:3004`。健康检查：

```powershell
Invoke-RestMethod http://localhost:3004/health
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

Rust 格式与静态检查：

```powershell
npm run fmt --workspace @open-guandan/server
npm run clippy --workspace @open-guandan/server
```

## 文档

- [掼蛋规则](./docs/rules.md)
- [后端架构与接口](./docs/backend.md)
- [牌型中英文名称](./docs/card-types.json)
