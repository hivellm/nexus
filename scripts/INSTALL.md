# Nexus Installation Scripts

Scripts de instalação automatizada para o Nexus, similar ao Bun, que instalam o serviço e o CLI diretamente do GitHub.

## Linux/macOS

```bash
curl -fsSL https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.sh | bash
```

### O que o script faz:

1. **Detecta a plataforma** (Linux x86_64/aarch64 ou macOS)
2. **Baixa a versão mais recente** do GitHub Releases
3. **Instala o binário** em `/usr/local/bin/nexus-server`
4. **Cria serviço systemd** (Linux) ou launchd (macOS)
5. **Configura auto-restart** com o sistema
6. **Adiciona ao PATH** automaticamente

### Variáveis de ambiente opcionais:

```bash
# Customizar diretório de instalação
export NEXUS_INSTALL_DIR="/opt/nexus"
curl -fsSL https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.sh | bash

# Customizar diretório de dados
export NEXUS_DATA_DIR="/var/lib/nexus-custom"
curl -fsSL https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.sh | bash
```

### Gerenciamento do serviço (Linux):

```bash
# Ver status
sudo systemctl status nexus

# Reiniciar
sudo systemctl restart nexus

# Parar
sudo systemctl stop nexus

# Ver logs
sudo journalctl -u nexus -f
```

## Windows

```powershell
powershell -c "irm https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.ps1 | iex"
```

### O que o script faz:

1. **Detecta a arquitetura** (x86_64 ou aarch64)
2. **Baixa a versão mais recente** do GitHub Releases
3. **Instala o binário** em `C:\Program Files\Nexus\nexus-server.exe`
4. **Cria serviço Windows** com auto-restart
5. **Adiciona ao PATH** do sistema
6. **Inicia o serviço** automaticamente

### Requisitos:

- PowerShell executado como **Administrador**
- Acesso à internet para baixar do GitHub

### Gerenciamento do serviço (Windows):

```powershell
# Ver status
Get-Service -Name Nexus

# Reiniciar
Restart-Service -Name Nexus

# Parar
Stop-Service -Name Nexus

# Ver logs
Get-EventLog -LogName Application -Source Nexus -Newest 50
```

## Estrutura de arquivos instalados

### Linux:

```
/usr/local/bin/nexus-server          # Binário do servidor
/etc/systemd/system/nexus.service  # Arquivo de serviço
/var/lib/nexus/               # Dados do serviço
/var/log/nexus/              # Logs do serviço
```

### macOS:

```
/usr/local/bin/nexus-server          # Binário do servidor
~/Library/LaunchAgents/com.hivellm.nexus.plist  # Serviço launchd
~/Library/Application Support/Nexus/  # Dados (se customizado)
```

### Windows:

```
C:\Program Files\Nexus\nexus-server.exe  # Binário do servidor
C:\ProgramData\Nexus\                   # Dados do serviço
C:\ProgramData\Nexus\logs\              # Logs do serviço
```

## Verificação

Após a instalação, verifique se tudo está funcionando:

```bash
# Verificar versão do servidor
nexus-server --version

# Verificar status do serviço (Linux)
sudo systemctl status nexus

# Verificar status do serviço (Windows)
Get-Service -Name Nexus

# Testar o servidor
curl http://localhost:15474/health
```

## Desinstalação

### Linux:

```bash
# Parar e desabilitar serviço
sudo systemctl stop nexus
sudo systemctl disable nexus
sudo rm /etc/systemd/system/nexus.service
sudo systemctl daemon-reload

# Remover binário
sudo rm /usr/local/bin/nexus-server

# Remover dados (opcional)
sudo rm -rf /var/lib/nexus
sudo rm -rf /var/log/nexus
```

### macOS:

```bash
# Parar e remover serviço
launchctl unload ~/Library/LaunchAgents/com.hivellm.nexus.plist
rm ~/Library/LaunchAgents/com.hivellm.nexus.plist

# Remover binário
rm /usr/local/bin/nexus-server
```

### Windows:

```powershell
# Parar e remover serviço
Stop-Service -Name Nexus
sc.exe delete Nexus

# Remover binário
Remove-Item "C:\Program Files\Nexus\nexus-server.exe" -Force

# Remover dados (opcional)
Remove-Item "C:\ProgramData\Nexus" -Recurse -Force
```

## Troubleshooting

### Linux: Serviço não inicia

```bash
# Ver logs detalhados
sudo journalctl -u nexus -n 50

# Verificar permissões
ls -la /usr/local/bin/nexus-server
sudo chmod +x /usr/local/bin/nexus-server

# Verificar configuração do serviço
sudo systemctl cat nexus
```

### Windows: Serviço não inicia

```powershell
# Ver eventos do serviço
Get-EventLog -LogName Application -Source Nexus -Newest 20

# Verificar se o binário existe
Test-Path "C:\Program Files\Nexus\nexus-server.exe"

# Verificar permissões
Get-Acl "C:\Program Files\Nexus\nexus-server.exe"
```

### CLI não encontrado

```bash
# Linux/macOS: Verificar PATH
echo $PATH | grep -i nexus
which nexus-server

# Windows: Verificar PATH
$env:Path -split ';' | Select-String -Pattern "Nexus"
Get-Command nexus-server -ErrorAction SilentlyContinue
```

## Desenvolvimento

Para testar os scripts localmente:

```bash
# Linux/macOS
bash scripts/install.sh

# Windows
powershell -ExecutionPolicy Bypass -File scripts/install.ps1
```

## Notas

- Os scripts baixam diretamente do GitHub Releases (sem domínio customizado)
- O serviço é configurado para reiniciar automaticamente após falhas
- O CLI é adicionado ao PATH automaticamente
- Os dados são persistidos em diretórios padrão do sistema
- O servidor estará disponível em `http://localhost:15474` após a instalação
