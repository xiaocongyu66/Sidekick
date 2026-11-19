#!/usr/bin/env bash
# 拉取开发依赖：nsh 前端框架源码（companion 以 path 依赖引用它）。
set -euo pipefail
cd "$(dirname "$0")"

if [ ! -d nsh-src ]; then
  echo "克隆 nsh → nsh-src/"
  git clone --depth 1 https://github.com/longbridge/nsh.git nsh-src
else
  echo "nsh-src/ 已存在，跳过"
fi
