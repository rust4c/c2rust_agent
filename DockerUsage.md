## 使用说明（fish shell）

1) 重新构建并启动
```fish
# 在仓库根目录
docker compose build
docker compose up -d
```

2) 确认容器运行
```fish
docker compose ps
```

3) 通过 SSH 登录容器
- 用户名：`agent`
- 初始密码：`agent`
- 端口映射：宿主机 `2222` -> 容器 `22`
```fish
ssh agent@localhost -p 2222
# 首次连接可能提示主机指纹，输入 yes
# 输入密码：agent
```

4) 安全建议：登录后立刻修改密码
```fish
# 在容器内
passwd agent
```

5) 可选：使用公钥方式免密登录
- 在宿主机上复制你的公钥到容器用户
```fish
# 将本机公钥追加到容器 agent 用户
# 如果没有公钥，先在宿主机生成：ssh-keygen -t ed25519 -C "you@example.com"
set -l pubkey (cat ~/.ssh/id_ed25519.pub)
ssh -p 2222 agent@localhost "mkdir -p ~/.ssh && chmod 700 ~/.ssh && printf '%s\n' '$pubkey' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"
```
- 完成后可禁用密码登录（更安全）。编辑容器 sshd_config，把 `PasswordAuthentication no`，然后重启 sshd：
```fish
# 容器内
sudo sed -i 's/^PasswordAuthentication .*/PasswordAuthentication no/' /etc/ssh/sshd_config
sudo kill -HUP (pidof sshd)
```
如容器没有 `sudo`，用 root 权限操作或在 compose 中临时执行。

6) 停止与清理
```fish
docker compose down       # 保留数据卷
# 或
docker compose down -v    # 连同 qdrant/sqlite 数据卷一起删除
```

## 注意事项
- 你当前的 docker-compose.yml 将配置文件挂载到 config.toml，SQLite 数据目录到 `/opt/c2rust_agent/data`，与附带的 Rust 配置读取路径相匹配即可。如果应用在容器中需要读取 config.toml 或 `/workspace/data`，可按需调整挂载路径和 config.toml 内的 `sqlite.path`。
- 如需只使用已经发布的 `c2rust_agent:latest` 镜像，且不从本地 Dockerfile 构建，可注释掉 `build:` 段落。