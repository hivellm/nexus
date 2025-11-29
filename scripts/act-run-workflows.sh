#!/bin/bash
# Script to run GitHub Actions workflows locally using act
# 
# Usage:
#   ./scripts/act-run-workflows.sh              # List available workflows
#   ./scripts/act-run-workflows.sh rust-tests  # Run rust-tests job
#   ./scripts/act-run-workflows.sh lint        # Run lint job
#   ./scripts/act-run-workflows.sh codespell   # Run codespell job
#   ./scripts/act-run-workflows.sh all        # Run all jobs

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Project directory
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

# Check if act is available
ACT_CMD="./act"
if [ ! -f "$ACT_CMD" ]; then
    echo -e "${YELLOW}act not found. Downloading...${NC}"
    curl -sL https://github.com/nektos/act/releases/latest/download/act_Linux_x86_64.tar.gz | tar -xz
    chmod +x act
fi

# Configure Docker
if [ -z "$DOCKER_HOST" ]; then
    # Try different Docker configurations
    if [ -S /var/run/docker.sock ]; then
        export DOCKER_HOST=unix:///var/run/docker.sock
    elif [ -S "$HOME/.docker/run/docker.sock" ]; then
        export DOCKER_HOST=unix://"$HOME/.docker/run/docker.sock"
    fi
fi

# Check if Docker is accessible
if ! docker info > /dev/null 2>&1; then
    echo -e "${RED}ERROR: Docker is not accessible.${NC}"
    echo "Make sure Docker Desktop is running."
    echo ""
    echo "On Windows with WSL, you may need to:"
    echo "  1. Open Docker Desktop"
    echo "  2. Go to Settings > General > Enable integration with my default WSL distro"
    echo "  3. Select Ubuntu-24.04"
    exit 1
fi

# Docker image compatible with GitHub Actions
ACT_IMAGE="ghcr.io/catthehacker/ubuntu:act-latest"

# Function to list workflows
list_workflows() {
    echo -e "${GREEN}Available workflows:${NC}"
    echo ""
    "$ACT_CMD" -l
    echo ""
    echo -e "${YELLOW}To run a specific job:${NC}"
    echo "  ./scripts/act-run-workflows.sh <job-name>"
    echo ""
    echo -e "${YELLOW}Examples:${NC}"
    echo "  ./scripts/act-run-workflows.sh rust-tests"
    echo "  ./scripts/act-run-workflows.sh lint"
    echo "  ./scripts/act-run-workflows.sh codespell"
}

# Function to run a specific job
run_job() {
    local job_name=$1
    echo -e "${GREEN}Running job: ${job_name}${NC}"
    echo ""
    
    "$ACT_CMD" -j "$job_name" \
        --container-architecture linux/amd64 \
        --image ubuntu-latest="$ACT_IMAGE" \
        --pull=false \
        --rm
}

# Function to run all jobs
run_all() {
    echo -e "${GREEN}Running all jobs...${NC}"
    echo ""
    
    # Run each job sequentially
    for job in rust-tests lint codespell; do
        echo -e "${YELLOW}=== Running $job ===${NC}"
        run_job "$job"
        echo ""
    done
}

# Main
if [ $# -eq 0 ]; then
    list_workflows
elif [ "$1" == "all" ]; then
    run_all
elif [ "$1" == "list" ] || [ "$1" == "-l" ]; then
    list_workflows
else
    run_job "$1"
fi
