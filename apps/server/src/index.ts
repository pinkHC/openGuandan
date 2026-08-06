import "dotenv/config";
import { buildApplication } from "./app.js";
import { loadConfig } from "./config.js";

const config = loadConfig();
const { app } = await buildApplication(config);

const shutdown = async (): Promise<void> => {
  await app.close();
  process.exit(0);
};

process.on("SIGINT", () => void shutdown());
process.on("SIGTERM", () => void shutdown());

try {
  await app.listen({ host: config.host, port: config.port });
} catch (error) {
  app.log.error(error);
  process.exit(1);
}
