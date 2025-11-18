#!/bin/bash

# Script para gerenciar servidor Nexus
# Fecha todos os processos nexus-server e inicia apenas um

echo "🛑 Parando todos os processos nexus-server..."

# Kill all nexus-server processes
pkill -9 -f nexus-server 2>/dev/null || true

# Espera um pouco para garantir que todos foram fechados
sleep 2

# Verifica se ainda há processos rodando
if pgrep -f nexus-server > /dev/null; then
    echo "❌ Ainda há processos rodando, tentando forçar parada..."
    pkill -9 -f nexus-server 2>/dev/null || true
    sleep 1
fi

# Verifica novamente
if pgrep -f nexus-server > /dev/null; then
    echo "❌ Falha ao parar todos os processos. Abortando."
    exit 1
fi

echo "✅ Todos os processos nexus-server foram parados."

# Go to project directory
cd /mnt/f/Node/hivellm/nexus

echo "🚀 Iniciando novo servidor..."

# Inicia o servidor em background
./target/release/nexus-server &
SERVER_PID=$!

echo "📝 PID do servidor: $SERVER_PID"

# Espera o servidor iniciar
sleep 5

# Verifica se o servidor está respondendo
if curl -s http://localhost:15474/health | grep -q "Healthy"; then
    echo "✅ Servidor iniciado com sucesso!"
    echo "🌐 Servidor rodando em: http://localhost:15474"
    echo "📊 PID: $SERVER_PID"
    echo ""
    echo "💡 Para parar o servidor, execute: kill $SERVER_PID"
    echo "💡 Ou execute este script novamente para reiniciar"
else
    echo "❌ Servidor não respondeu no health check"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi

# Keep script running to avoid killing the server
echo "🔄 Servidor rodando em background. Pressione Ctrl+C para parar."
trap "echo '🛑 Parando servidor...'; kill $SERVER_PID 2>/dev/null || true; exit 0" INT
while true; do
    sleep 1
done
