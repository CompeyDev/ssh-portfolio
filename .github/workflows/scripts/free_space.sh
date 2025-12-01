#!/usr/bin/env bash

# From: https://github.com/apache/arrow/blob/82324f/ci/scripts/util_free_space.sh

#
# Licensed to the Apache Software Foundation (ASF) under one
# or more contributor license agreements.  See the NOTICE file
# distributed with this work for additional information
# regarding copyright ownership.  The ASF licenses this file
# to you under the Apache License, Version 2.0 (the
# "License"); you may not use this file except in compliance
# with the License.  You may obtain a copy of the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing,
# software distributed under the License is distributed on an
# "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
# KIND, either express or implied.  See the License for the
# specific language governing permissions and limitations
# under the License.

set -eux

dir_stats() {
  echo "::group::Quick stats for path(s) $@"
  du -h --apparent-size --max-depth=1 "$@" 2>/dev/null || true
  echo "::endgroup::"
}

df -h
dir_stats /usr/local/*

# ~1GB
sudo rm -rf \
  /usr/local/aws-sam-cli \
  /usr/local/julia* || :

dir_stats /usr/local/bin/*

# ~1GB (From 1.2GB to 214MB)
sudo rm -rf \
  /usr/local/bin/aliyun \
  /usr/local/bin/azcopy \
  /usr/local/bin/bicep \
  /usr/local/bin/cmake-gui \
  /usr/local/bin/cpack \
  /usr/local/bin/helm \
  /usr/local/bin/hub \
  /usr/local/bin/kubectl \
  /usr/local/bin/minikube \
  /usr/local/bin/node \
  /usr/local/bin/packer \
  /usr/local/bin/pulumi* \
  /usr/local/bin/sam \
  /usr/local/bin/stack \
  /usr/local/bin/terraform || :

# 142M
sudo rm -rf /usr/local/bin/oc || : \

dir_stats /usr/local/share/*

# 506MB
sudo rm -rf /usr/local/share/chromium || :
# 1.3GB
sudo rm -rf /usr/local/share/powershell || :

dir_stats /usr/local/lib/*

# 15GB
sudo rm -rf /usr/local/lib/android || :
# 341MB
sudo rm -rf /usr/local/lib/heroku || :
# 1.2GB
sudo rm -rf /usr/local/lib/node_modules || :

dir_stats /opt/*

# 679MB
sudo rm -rf /opt/az || :

dir_stats /opt/microsoft/*

# 197MB
sudo rm -rf /opt/microsoft/powershell || :

dir_stats /opt/hostedtoolcache/*

# 5.3GB
sudo rm -rf /opt/hostedtoolcache/CodeQL || :
# 1.4GB
sudo rm -rf /opt/hostedtoolcache/go || :
# 489MB
sudo rm -rf /opt/hostedtoolcache/PyPy || :
# 376MB
sudo rm -rf /opt/hostedtoolcache/node || :

# Remove Web browser packages
sudo apt-get purge -y firefox
# google-chrome-stable isn't installed on arm64 image.
sudo apt-get purge -y google-chrome-stable || :
# microsoft-edge-stable isn't installed on arm64 image.
sudo apt-get purge -y microsoft-edge-stable || :

df -h
