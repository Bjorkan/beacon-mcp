import { createServer } from "node:http";

createServer((request, response) => {
  response.setHeader("content-type", "application/json");
  if (request.url?.startsWith("/api/v1/nodes")) {
    response.end(
      JSON.stringify({
        items: [{ id: "container-node", name: "Container test" }],
        hasMore: false,
      }),
    );
    return;
  }
  response.end("[]");
}).listen(8080, "0.0.0.0");
