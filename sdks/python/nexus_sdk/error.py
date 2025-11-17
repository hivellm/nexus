"""Error types for Nexus SDK."""


class NexusError(Exception):
    """Base exception for all Nexus SDK errors."""

    pass


class HttpError(NexusError):
    """HTTP request error."""

    def __init__(self, message: str, status_code: int = None):
        super().__init__(message)
        self.status_code = status_code


class ApiError(NexusError):
    """API error response."""

    def __init__(self, message: str, status: int):
        super().__init__(f"API error: {message} (status: {status})")
        self.message = message
        self.status = status


class AuthenticationError(NexusError):
    """Authentication failed."""

    pass


class ConfigurationError(NexusError):
    """Invalid configuration."""

    pass


class ConnectionError(NexusError):
    """Connection error."""

    pass


class NetworkError(NexusError):
    """Network error."""

    pass


class TimeoutError(NexusError):
    """Request timeout."""

    pass


class InvalidResponseError(NexusError):
    """Invalid response format."""

    pass


class ValidationError(NexusError):
    """Validation error."""

    pass

