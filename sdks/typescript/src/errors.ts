import type { NexusError as NexusErrorType } from './types';

/**
 * Custom error class for Nexus SDK errors
 */
export class NexusSDKError extends Error {
  public readonly code?: string;
  public readonly details?: unknown;
  public readonly statusCode?: number;

  constructor(message: string, code?: string, details?: unknown, statusCode?: number) {
    super(message);
    this.name = 'NexusSDKError';
    this.code = code;
    this.details = details;
    this.statusCode = statusCode;
    Error.captureStackTrace(this, this.constructor);
  }

  /**
   * Create error from server response
   */
  static fromServerError(error: NexusErrorType, statusCode?: number): NexusSDKError {
    return new NexusSDKError(
      error.message,
      error.code,
      error.details,
      statusCode
    );
  }

  /**
   * Create error from Axios error
   */
  static fromAxiosError(error: unknown): NexusSDKError {
    if (typeof error === 'object' && error !== null && 'response' in error) {
      const axiosError = error as {
        response?: {
          status: number;
          data?: { error?: NexusErrorType };
        };
        message: string;
      };

      if (axiosError.response?.data?.error) {
        return NexusSDKError.fromServerError(
          axiosError.response.data.error,
          axiosError.response.status
        );
      }

      return new NexusSDKError(
        axiosError.message,
        undefined,
        undefined,
        axiosError.response?.status
      );
    }

    if (error instanceof Error) {
      return new NexusSDKError(error.message);
    }

    return new NexusSDKError('Unknown error occurred');
  }
}

/**
 * Authentication error
 */
export class AuthenticationError extends NexusSDKError {
  constructor(message: string) {
    super(message, 'AUTHENTICATION_ERROR', undefined, 401);
    this.name = 'AuthenticationError';
  }
}

/**
 * Connection error
 */
export class ConnectionError extends NexusSDKError {
  constructor(message: string) {
    super(message, 'CONNECTION_ERROR');
    this.name = 'ConnectionError';
  }
}

/**
 * Query execution error
 */
export class QueryExecutionError extends NexusSDKError {
  constructor(message: string, details?: unknown) {
    super(message, 'QUERY_EXECUTION_ERROR', details);
    this.name = 'QueryExecutionError';
  }
}

/**
 * Validation error
 */
export class ValidationError extends NexusSDKError {
  constructor(message: string) {
    super(message, 'VALIDATION_ERROR', undefined, 400);
    this.name = 'ValidationError';
  }
}

