#!/bin/bash
# Run tests in Ubuntu Docker container (simulates GitHub Actions environment)
# Usage: ./scripts/test-docker-ubuntu.sh [--build] [--no-cache] [--filter <test_name>]

set -e

IMAGE_NAME="nexus-test-ubuntu"
DOCKERFILE_PATH="scripts/docker/Dockerfile.test-ubuntu"
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

BUILD=false
NO_CACHE=""
TEST_FILTER=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --build|-b)
            BUILD=true
            shift
            ;;
        --no-cache)
            NO_CACHE="--no-cache"
            shift
            ;;
        --filter|-f)
            TEST_FILTER="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "========================================"
echo "  Nexus Test Runner - Ubuntu Docker"
echo "========================================"
echo ""

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "ERROR: Docker is not running. Please start Docker."
    exit 1
fi

# Check if image exists or build flag is set
IMAGE_EXISTS=$(docker images -q "$IMAGE_NAME" 2>/dev/null)
if [[ -z "$IMAGE_EXISTS" ]] || [[ "$BUILD" == "true" ]]; then
    echo "Building Docker image: $IMAGE_NAME"
    docker build -f "$DOCKERFILE_PATH" -t "$IMAGE_NAME" $NO_CACHE "$PROJECT_ROOT"
    echo "Docker image built successfully!"
fi

echo ""
echo "Running tests in Ubuntu container..."
echo ""

# Prepare test command
TEST_CMD="cargo nextest run --workspace --no-default-features"
if [[ -n "$TEST_FILTER" ]]; then
    TEST_CMD="$TEST_CMD -E 'test($TEST_FILTER)'"
fi

# Run tests in Docker
docker run --rm \
    -v "$PROJECT_ROOT:/workspace" \
    -w /workspace \
    "$IMAGE_NAME" \
    bash -c "$TEST_CMD"

EXIT_CODE=$?

echo ""
if [[ $EXIT_CODE -eq 0 ]]; then
    echo "========================================"
    echo "  ALL TESTS PASSED!"
    echo "========================================"
else
    echo "========================================"
    echo "  TESTS FAILED (exit code: $EXIT_CODE)"
    echo "========================================"
fi

exit $EXIT_CODE

