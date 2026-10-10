import { defineConfig, type Plugin } from "vite";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createInterface } from "node:readline";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

function enginePreview(): Plugin {
  let child: ChildProcessWithoutNullStreams | undefined;
  let chain: Promise<unknown> = Promise.resolve();
  const pending: Array<{
    resolve: (data: unknown) => void;
    reject: (err: Error) => void;
  }> = [];
  const fail = (error: Error) => {
    for (const p of pending.splice(0)) p.reject(error);
    child = undefined;
  };
  function start() {
    if (child) return child;
    const executable =
      process.env.SIGNAL_ENGINE ||
      resolve(
        "target/debug",
        process.platform === "win32" ? "signal-engine.exe" : "signal-engine",
      );
    if (!existsSync(executable))
      throw new Error("Build the shared Rust engine first: npm run engine");
    child = spawn(executable, [], { windowsHide: true, env: process.env });
    createInterface({ input: child.stdout }).on("line", (line) => {
      const request = pending.shift();
      if (request) {
        try {
          request.resolve(JSON.parse(line));
        } catch {
          request.reject(new Error("Invalid engine response"));
        }
      }
    });
    child.stderr.on("data", (data) => process.stderr.write(data));
    child.on("error", fail);
    child.on("exit", () => fail(new Error("Rust engine exited")));
    return child;
  }
  function call(request: unknown) {
    const next = chain
      .catch(() => undefined)
      .then(
        () =>
          new Promise((resolveReply, reject) => {
            const proc = start();
            const timeout = setTimeout(() => {
              proc.kill();
              fail(new Error("Engine request timed out"));
            }, 12_000);
            pending.push({
              resolve: (value) => {
                clearTimeout(timeout);
                resolveReply(value);
              },
              reject: (error) => {
                clearTimeout(timeout);
                reject(error);
              },
            });
            proc.stdin.write(JSON.stringify(request) + "\n");
          }),
      );
    chain = next;
    return next;
  }
  return {
    name: "signal-rust-preview",
    configureServer(server) {
      server.middlewares.use("/api/signal", async (req, res) => {
        const origin = req.headers.origin;
        if (
          req.method !== "POST" ||
          !req.headers["content-type"]?.startsWith("application/json") ||
          (origin && origin !== "http://127.0.0.1:1420")
        ) {
          res.statusCode = 403;
          res.end("Loopback preview requests only");
          return;
        }
        try {
          let body = "";
          for await (const chunk of req) {
            body += chunk;
            if (body.length > 8192) throw new Error("Request too large");
          }
          const result = await call(JSON.parse(body));
          res.setHeader("Content-Type", "application/json");
          res.end(JSON.stringify(result));
        } catch (error) {
          res.statusCode = 503;
          res.setHeader("Content-Type", "application/json");
          res.end(
            JSON.stringify({
              ok: false,
              error:
                error instanceof Error ? error.message : "Engine unavailable",
            }),
          );
        }
      });
      server.httpServer?.on("close", () => {
        child?.stdin.end('{"action":"shutdown"}\n');
      });
    },
  };
}

export default defineConfig({
  plugins: [enginePreview()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: "127.0.0.1" },
});
