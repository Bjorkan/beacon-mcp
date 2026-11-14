"""Application failures shared by validation, HTTP reads and MCP results."""


class GatewayError(Exception):
    def __init__(
        self,
        code: str,
        message: str,
        *,
        status: int | None = None,
        upstream_code: str | None = None,
        retry_after: str | None = None,
    ) -> None:
        super().__init__(message)
        self.code: str = code
        self.message: str = message
        self.status: int | None = status
        self.upstream_code: str | None = upstream_code
        self.retry_after: str | None = retry_after


def invalid(message: str) -> GatewayError:
    return GatewayError("INVALID_ARGUMENT", message)
