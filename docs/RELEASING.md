# 发布流程

发布由 GitHub Actions 完成：**推送 `v*` tag 即发布**。工作流会构建 NSIS 安装包与便携版、跑测试、生成校验文件，并用 `docs/releases/<tag>.md` 作为 Release 正文。

## 1. 准备版本

三处版本号必须一致（工作流会校验，不一致直接失败）：

| 文件 | 字段 |
|---|---|
| `package.json` | `version` |
| `src-tauri/Cargo.toml` | `[package] version` |
| `src-tauri/tauri.conf.json` | `version` |

同时更新 `Cargo.lock`（`cargo update -p workscene --precise <version>` 或构建时自动刷新）。

## 2. 写发布说明

新建 `docs/releases/v<version>.md`，作为 GitHub Release 正文。建议结构：

```markdown
# WorkScene v1.2.0

> 发布日期：YYYY-MM-DD · Windows 10 / 11 (x64)

一句话概括本次更新。

## 本次更新
- ...

## 安装
（列出安装包与便携版文件名、SmartScreen 说明）

## 已知限制
```

并把它加入 `docs/RELEASE.md` 的版本表格。

## 3. 提交并打 tag

```powershell
pnpm build
cd src-tauri; cargo test --lib; cd ..

git add -A
git commit -m "release(v1.2.0): ..."
git tag v1.2.0
git push origin main
git push origin v1.2.0
```

## 4. 工作流做了什么

`.github/workflows/release.yml`（tag `v*` 或手动触发）：

1. 校验 tag 与三处版本号一致，且 `docs/releases/<tag>.md` 存在；
2. 安装 pnpm / Node 20 / Rust stable，`pnpm install --frozen-lockfile`；
3. `pnpm build`（前端类型检查 + 产物）与 `cargo test --lib`；
4. `pnpm tauri build` 生成 `WorkScene_<version>_x64-setup.exe`；
5. 用 `start.cmd` 打包 `WorkScene-<version>-portable.zip`；
6. 生成 `SHA256SUMS.txt`；
7. `gh release create --verify-tag` 创建 Release 并上传三个文件；已存在则覆盖上传并同步正文。

手动重发：Actions → **Release** → Run workflow，填入已存在的 tag。

## 5. 本地打包（可选）

```powershell
pnpm tauri build
# 产物：
# src-tauri/target/release/bundle/nsis/WorkScene_<version>_x64-setup.exe
# src-tauri/target/release/workscene.exe
```

安装包未签名；如需签名，见 [`docs/PACKAGING.md`](PACKAGING.md)。

## 6. 历史版本

- **不要删除已发布的 tag / Release**：新版本发布不会影响旧版本；工作流只会对**同名 tag** `--clobber` 覆盖资产。
- 每个新版本的 `docs/releases/<tag>.md` 末尾保留「历史版本」链接，避免用户只能看到 Latest。
- 升级兼容性写在发布说明的「升级说明」里；数据库迁移只追加，不修改历史 migration。
- 历史版本统一入口：<https://github.com/fanfanhappy/workscene/releases>

## 7. 回滚

- 删除 Release：`gh release delete <tag> --yes`（仅限误发/空包，正常历史版本不要删）。
- 重新发布：修复后重跑工作流（会覆盖上传同名资产）。
- 不要移动已发布的 tag；如需修复版本，发一个新的补丁版本。
