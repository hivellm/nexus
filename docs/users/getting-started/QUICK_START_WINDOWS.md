---
title: Quick Start (Windows)
module: getting-started
id: quick-start-windows
order: 6
description: Windows-specific quick start guide
tags: [windows, quick-start, getting-started]
---

# Quick Start (Windows)

Windows-specific guide to get started with Nexus.

## Prerequisites

- Windows 10/11 or Windows Server
- PowerShell 5.1+ or PowerShell 7+
- Administrator privileges (for service installation)

## Installation

### Automated Installation

```powershell
powershell -c "irm https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.ps1 | iex"
```

### Manual Installation

```powershell
# Download latest release
Invoke-WebRequest -Uri "https://github.com/hivellm/nexus/releases/latest/download/nexus-server-windows-x64.exe" -OutFile "nexus-server.exe"

# Move to PATH location
Move-Item nexus-server.exe "$env:USERPROFILE\.cargo\bin\nexus-server.exe"
```

## Service Management

### Check Service Status

```powershell
Get-Service -Name Nexus
```

### Start Service

```powershell
Start-Service -Name Nexus
```

### Stop Service

```powershell
Stop-Service -Name Nexus
```

### Restart Service

```powershell
Restart-Service -Name Nexus
```

## First Query

### Using PowerShell

```powershell
$body = @{
    query = "MATCH (n) RETURN n LIMIT 10"
} | ConvertTo-Json

Invoke-RestMethod -Uri "http://localhost:15474/cypher" `
    -Method POST `
    -ContentType "application/json" `
    -Body $body
```

### Using curl (if installed)

```powershell
curl.exe -X POST http://localhost:15474/cypher `
    -H "Content-Type: application/json" `
    -d '{\"query\": \"MATCH (n) RETURN n LIMIT 10\"}'
```

## Configuration

### Environment Variables

```powershell
# Set environment variables
$env:NEXUS_BIND_ADDR = "0.0.0.0:15474"
$env:NEXUS_ROOT_USERNAME = "admin"
$env:NEXUS_ROOT_PASSWORD = "secure_password"
```

### Config File Location

```
C:\ProgramData\Nexus\config.yml
```

## Logs

### View Logs

```powershell
# View recent logs
Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100

# Follow logs
Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100 -Wait
```

### Log Location

```
C:\ProgramData\Nexus\logs\nexus.log
```

## Troubleshooting

### Port Already in Use

```powershell
# Check what's using port 15474
netstat -ano | findstr :15474

# Kill process (replace PID with actual process ID)
taskkill /PID <PID> /F
```

### Service Won't Start

```powershell
# Check service status
Get-Service -Name Nexus

# View event logs
Get-EventLog -LogName Application -Source Nexus -Newest 10
```

## Related Topics

- [Installation Guide](./INSTALLATION.md) - General installation
- [Quick Start Guide](./QUICK_START.md) - Cross-platform guide
- [Service Management](../operations/SERVICE_MANAGEMENT.md) - Service management

