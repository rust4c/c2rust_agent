# 赛事手册

为了提升转译工具的通用性，本次比赛初赛会考察转译作品对两个 C 语言工程项目（详见目录 [translate_chibicc](./translate_chibicc) 和 [translate_littlefs_fuse](./translate_littlefs_fuse)
）转译为 Rust 语言工程项目的能力，即会将转译 translate_chibicc 和 translate_littlefs_fuse 得到的分数做加和，作为参赛作品的工程实现评分。工程实现评分细则可以在[第三届vivo蓝河操作系统创新赛主页](https://competition-web.atomgit.com/competitionInfo?id=49f0205ecd5c81c96381829456fef6a5#heading-0-0)的“评审机制”章节的“评分标准”子章节查询。

## 作品源代码结构规范
每个仓库的正式分支需要包含
```
.
├── demo
├── Dockerfile
├── docs
├── LICENSE
└── README.md
```
其中 `demo` 目录包含该作品的工作演示视频，保存格式为 mp4 或者 webm ；`Dockerfile` 文件包含制作评测 docker 镜像的代码； `docs` 目录包含该作品参与答辩使用的演示文稿(ppt)、Keynotes 或者 PDF 文档； `LICENSE` 包含该项目的开源协议，推荐使用 [Mulan PSL v2](https://license.coscl.org.cn/MulanPSL2) ； `README.md` 为该项目的概述。

## 提交规范
我们为每支参赛队伍准备了一台云服务器，每支参赛队伍需要把成果以 docker 镜像的形式提交到这台云服务器上。我们提供了一个[样例 Dockerfile](./Dockerfile)。在家目录下需要放置一个配置文件，起名为`problem.toml`。

### problem.toml
参赛队伍需要提交到云主机家目录的 `problem.toml` 格式如下
```toml
[problem.foobar]
# 需要进行评测的 docker image id
docker_image = "debian:latest"
# docker 镜像里代码生成阶段的工作目录。
codegen_workdir = "/tmp"
# docker 镜像里代码生成阶段需要运行的命令（非Shell命令，不能带有管道等Shell连接操作符）。
codegen_command = "ls -l"
# docker 镜像里代码生成阶段日志输出路径。
codegen_logfile = "/tmp/run.log"
# docker 镜像里生成的代码所在的目录。
codegen_resultdir = "/tmp"
# docker 镜像里构建生成的代码的命令，并假设工作目录是 codegen_resultdir。
build_command = "touch translated_foobar"
# docker 镜像里生成的二进制可执行文件路径。
exe = "/tmp/translated_foobar"
```
其中 `foobar` 为问题 ID。样例文件可见 [problem.example.toml](./problem.example.toml)。
本次赛题要求的是制作一个 C 语言工程到 Rust 语言工程的转译工具，所以在上述 `codegen_resultdir` 里应该放置的是一个 Cargo 工程项目，并能使用 `cargo` 命令进行构建。

### 评测流程
以上述 `problem.toml` 为例，评测程序首先会利用 `problem.foobar.docker_image` 创建一个新的容器。切换到容器内的 `problem.foobar.codegen_workdir` 下，运行命令 `problem.foobar.codegen_command`。这一阶段结束后，切换到 `problem.foobar.resultdir` 目录下，运行命令 `problem.foobar.build_command`。最后我们期望一个应用可执行文件生成在 `problem.foobar.exe` 这个路径下。评测程序会使用这个 `problem.foobar.exe` 运行一些测试用例，来检查转译后的程序的功能正确性。


## FAQ
Q: 本次比赛的初赛需要转译几个项目？

A: chibicc 和 littlefs_fuse 都需要进行转译，我们希望转译工具能达到一定程度的通用性。工程实现总分50分，两个项目各占25分。即两个项目平分工程实现各个子评分项的分值。

Q: 转译作品的开发语言和环境有何要求？

A: 开发语言不限，但评测阶段需要在赛事方提供的ubuntu 24.04 云主机的docker环境下部署自己的作品。

Q: 赛事方提供的云主机能作为作品的开发环境么？

A: 赛事方提供的云主机主要用来提交 docker 镜像以及运行功能评测，建议开发者自行搭建开发环境。

Q: 为什么需要基于docker部署作品？

A: 这样可以最大程度让参赛者本地开发部署的效果和赛事方验证时的效果一致，避免因为环境问题影响参赛者的实现效果。

Q: 转译作品的代码可以使用大模型能力吗？

A: 可以使用公开的大模型能力，但是赛事方不提供大模型 API 资源。