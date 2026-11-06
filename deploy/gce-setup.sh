#!/usr/bin/env bash
set -euo pipefail

echo "=========================================================="
echo "⚡ Tuning Linux Kernel Network Stack for AeroKV (GCE VM)"
echo "=========================================================="

# 1. Increase UDP & TCP socket buffer limits for HTTP/3 QUIC & RESP
sudo sysctl -w net.core.rmem_max=26214400
sudo sysctl -w net.core.wmem_max=26214400
sudo sysctl -w net.core.rmem_default=26214400
sudo sysctl -w net.core.wmem_default=26214400

# 2. Increase maximum open connection backlog
sudo sysctl -w net.core.somaxconn=65535
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=65535

# 3. Enable fast TCP TIME_WAIT reuse
sudo sysctl -w net.ipv4.tcp_tw_reuse=1

# 4. Set persistent sysctl parameters
cat <<EOF | sudo tee /etc/sysctl.d/99-aerokv.conf
net.core.rmem_max = 26214400
net.core.wmem_max = 26214400
net.core.rmem_default = 26214400
net.core.wmem_default = 26214400
net.core.somaxconn = 65535
net.ipv4.tcp_max_syn_backlog = 65535
net.ipv4.tcp_tw_reuse = 1
EOF

echo "✅ Sysctl tuning applied successfully."

