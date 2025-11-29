namespace Nexus.SDK;

/// <summary>
/// Base exception for Nexus SDK errors.
/// </summary>
public class NexusException : Exception
{
    public NexusException(string message) : base(message)
    {
    }

    public NexusException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}

/// <summary>
/// Exception thrown when an HTTP request to the Nexus API fails.
/// </summary>
public class NexusApiException : NexusException
{
    /// <summary>
    /// HTTP status code.
    /// </summary>
    public int StatusCode { get; }

    /// <summary>
    /// Response body.
    /// </summary>
    public string ResponseBody { get; }

    public NexusApiException(int statusCode, string responseBody)
        : base($"Nexus API error: HTTP {statusCode}: {responseBody}")
    {
        StatusCode = statusCode;
        ResponseBody = responseBody;
    }
}

/// <summary>
/// Exception thrown when a transaction operation fails.
/// </summary>
public class NexusTransactionException : NexusException
{
    public NexusTransactionException(string message) : base(message)
    {
    }

    public NexusTransactionException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}
