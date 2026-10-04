const port = process.env.PORT ?? "3000";
try {
  const response = await fetch(`http://127.0.0.1:${port}/healthz`, {
    signal: AbortSignal.timeout(2000),
  });
  const body = await response.json();
  if (!response.ok || body.status !== "ok") process.exit(1);
} catch {
  process.exit(1);
}
