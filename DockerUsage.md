## Usage Instructions (fish shell)

1) Rebuild and start

```fish
# In the repository root directory
docker compose up -d
```

2) Confirm container is running

```fish
docker compose ps
```

3) Login to container via SSH

- Username: `agent`
- Initial password: `agent`
- Port mapping: Host `2222` -> Container `22`

```fish
ssh agent@localhost -p 2222
# First connection may prompt for host fingerprint, enter yes
# Enter password: agent
```

4) Security recommendation: Change password immediately after login

```fish
# Inside the container
passwd agent
```

5) Optional: Use public key for passwordless login

- Copy your public key from host machine to container user

```fish
# Append local machine's public key to container agent user
# If no public key exists, generate one on host first: ssh-keygen -t ed25519 -C "you@example.com"
set -l pubkey (cat ~/.ssh/id_ed25519.pub)
ssh -p 2222 agent@localhost "mkdir -p ~/.ssh && chmod 700 ~/.ssh && printf '%s\n' '$pubkey' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"
```

- After completion, you can disable password login (more secure). Edit container sshd_config, set `PasswordAuthentication no`, then restart sshd:

```fish
# Inside container
sudo sed -i 's/^PasswordAuthentication .*/PasswordAuthentication no/' /etc/ssh/sshd_config
sudo kill -HUP (pidof sshd)
```

If container doesn't have `sudo`, operate with root privileges or execute temporarily in compose.

6) Stop and cleanup

```fish
docker compose down       # Keep data volumes
# or
docker compose down -v    # Delete qdrant/sqlite data volumes together
```

## Notes

- Your current docker-compose.yml mounts the config file to config.toml, SQLite data directory to `/opt/c2rust_agent/data`, which should match the accompanying Rust configuration read paths. If the application needs to read config.toml or `/workspace/data` in the container, adjust the mount paths and `sqlite.path` in config.toml as needed.
- If you only want to use the already published `c2rust_agent:latest` image without building from local Dockerfile, you can comment out the `build:` section.