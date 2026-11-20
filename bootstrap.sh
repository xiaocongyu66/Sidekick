#!/usr/bin/env bash
# 拉取开发依赖：nsh 前端框架源码（companion 以 path 依赖引用它）。
# 固定 tag：nsh CLI 与 nsh crates 的版本必须一致（CI 的 nsh 取自 release）。
NSH_TAG="${NSH_TAG:-v0.6.1}"
set -euo pipefail
cd "$(dirname "$0")"

if [ ! -d nsh-src ]; then
  echo "克隆 nsh $NSH_TAG → nsh-src/"
  git clone --depth 1 --branch "$NSH_TAG" https://github.com/longbridge/nsh.git nsh-src
else
  echo "nsh-src/ 已存在，跳过"
fi
