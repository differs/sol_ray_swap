# sol_ray_swap Substreams modules

This package was initialized via `substreams init`, using the `sol-hello-world` template.

## Usage

```bash
substreams build
substreams auth
substreams gui       			  # Get streaming!
```

Optionally, you can publish your Substreams to the [Substreams Registry](https://substreams.dev).

```bash
substreams registry login         # Login to substreams.dev
substreams registry publish       # Publish your Substreams to substreams.dev
```

## Modules

### `map_filtered_transactions`

This module retrieves Solana transactions filtered by one or several Program IDs.
You will only receive transactions containing the specified Program IDs).

**NOTE:** Transactions containing voting instructions will NOT be present.

### `map_my_data`

This module allows you to create transformations on the filtered transactions.

## 命令使用说明（中文）

本项目提供 Substreams 模块用于解析 Solana 区块中的 Raydium Swap 交易事件。以下为常用命令与参数说明。

### 一、先决条件

- 已安装 Rust，并添加 wasm 目标：
  - `rustup target add wasm32-unknown-unknown`
- 已安装 Substreams CLI（参见官方文档：https://substreams.streamingfast.io）
- 可访问 Solana 主网端点：`mainnet.sol.streamingfast.io:443`

### 二、构建

在项目根目录执行：

```bash
substreams build
```

成功后会在根目录生成打包文件，例如：`sol-ray-swap-v0.1.0.spkg`。

### 三、运行模块

运行 `map_ray_swap` 模块，从指定起始块处理 N 个区块：

```bash
substreams run -e mainnet.sol.streamingfast.io:443 sol-ray-swap-v0.1.0.spkg map_ray_swap -s <start_block> -t +<count>
```

示例：

```bash
substreams run -e mainnet.sol.streamingfast.io:443 sol-ray-swap-v0.1.0.spkg map_ray_swap -s 367034550 -t +3
```

### 四、参数说明

- `-e <endpoint>`：Substreams 端点（Solana 主网为 `mainnet.sol.streamingfast.io:443`）。
- `<package.spkg>`：`substreams build` 生成的包文件名。
- `<module_name>`：要运行的模块名，这里为 `map_ray_swap`。
- `-s <start_block>`：起始区块高度。
- `-t +N`：从起始区块向前处理 N 个区块。也可用 `-t <end_block>` 指定结束区块高度。

### 五、日志与调试

当检测到 Raydium Swap 时，模块会输出丰富的日志，帮助定位：

- `Full info`：完整 `meta.log_messages`（包含 `ray_log` 等信息）。
- `Full inner_instructions`、`Full pre/post_balances`、`pre/post_token_balances`：内部指令与余额快照。
- `Raydium Swap Accounts`：该次内联指令涉及的账户列表。
- `Instruction data length ... Amount in/out ...`：指令数据长度与推断的金额信息。

如需减少日志量，可在 `src/lib.rs` 中注释或删除相应的 `substreams::log::info!` 行。

### 六、输出数据结构

模块输出类型为 `io.blockchain.v1.dex.trade.TradeEvents`，包含一个或多个 `TradeEvent`，其 `trade` 字段内含：

- 代币地址：`tokenAAddress`、`tokenBAddress`
- 用户代币账户与所有者：`userATokenAccountAddress`、`userAAccountOwnerAddress` 等
- 交易数量：`userAAmount`、`userBAmount`
- 金库与池信息：`vaultA`、`vaultB`、`poolAddress`、`poolConfigAddress`
- 余额变更：`vaultAPreAmount`、`vaultAPostAmount` 等

### 七、常见问题

- 无法编译 wasm 目标：请先执行 `rustup target add wasm32-unknown-unknown`。
- 未获取到事件：可能该区块范围内没有 `SwapRaydiumV4/Instruction: Swap`，可调整 `-s`、`-t`。
- 输出过多：缩小区块范围或减少日志打印。
