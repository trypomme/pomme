#!/bin/bash
# Sets up a fresh Ubuntu 24.04 server as self-hosted GitHub Actions runners for the trypomme org.
# It is passed to Vultr as the server's start-up script ("user data"), so nobody needs to log in.
#
# Before use, replace the three values below:
#   RUNNER_TOKEN  a registration token from
#                 https://github.com/organizations/trypomme/settings/actions/runners/new
#                 (it expires after 1 hour, and is only used to register; it is not kept)
#   RUNNER_COUNT  how many runners (jobs at the same time) this server runs
#   RUNNER_NAME   the start of each runner's name; runner 1 is "<RUNNER_NAME>-1"
#
# Jobs pick these runners with:  runs-on: [self-hosted, vultr]
#
# Each runner has its own Linux user, so two jobs never share a Rust or Node install.
# Those users may use sudo without a password, because the workflows install packages with
# apt-get (like GitHub's own runners). Only use this server for private repos in the org.
set -euxo pipefail

RUNNER_TOKEN="__RUNNER_TOKEN__"
RUNNER_COUNT="__RUNNER_COUNT__"
RUNNER_NAME="__RUNNER_NAME__"
ORG_URL="https://github.com/trypomme"

export DEBIAN_FRONTEND=noninteractive

# Nothing listens for outside connections: the runner only connects out to GitHub.
# (The Vultr firewall also blocks all incoming traffic.)
systemctl disable --now ssh.socket ssh.service || true

# Extra memory for big Rust link steps.
if [ ! -f /swapfile ]; then
  fallocate -l 8G /swapfile && chmod 600 /swapfile && mkswap /swapfile && swapon /swapfile
  echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi

# Tools the workflows expect to find, as on GitHub's Ubuntu runners.
apt-get update
apt-get -y upgrade
apt-get install -y --no-install-recommends \
  build-essential pkg-config git curl ca-certificates jq zip unzip xz-utils nasm python3 \
  libssl-dev unattended-upgrades
# GitHub CLI (gh), used by the review and release jobs.
curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg -o /usr/share/keyrings/githubcli-archive-keyring.gpg
echo "deb [arch=amd64 signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
  > /etc/apt/sources.list.d/github-cli.list
apt-get update && apt-get install -y gh
# Security updates install themselves.
dpkg-reconfigure -f noninteractive unattended-upgrades

# The newest runner release.
version=$(curl -fsSL https://api.github.com/repos/actions/runner/releases/latest | jq -r .tag_name | sed 's/^v//')
curl -fsSL -o /tmp/runner.tar.gz \
  "https://github.com/actions/runner/releases/download/v${version}/actions-runner-linux-x64-${version}.tar.gz"

for i in $(seq 1 "$RUNNER_COUNT"); do
  user="runner$i"
  dir="/home/$user/actions-runner"
  id "$user" >/dev/null 2>&1 || useradd --create-home --shell /bin/bash "$user"
  echo "$user ALL=(ALL) NOPASSWD:ALL" > "/etc/sudoers.d/$user"
  chmod 440 "/etc/sudoers.d/$user"
  mkdir -p "$dir" && tar -xzf /tmp/runner.tar.gz -C "$dir" && chown -R "$user:$user" "$dir"
  "$dir/bin/installdependencies.sh"
  sudo -u "$user" -H "$dir/config.sh" --unattended --replace \
    --url "$ORG_URL" --token "$RUNNER_TOKEN" \
    --name "${RUNNER_NAME}-$i" --labels vultr --work _work
  (cd "$dir" && ./svc.sh install "$user" && ./svc.sh start)
done

rm -f /tmp/runner.tar.gz
echo "runner setup finished"
