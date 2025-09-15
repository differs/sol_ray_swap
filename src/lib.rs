mod pb {
    pub mod io {
        pub mod blockchain {
            pub mod v1 {
                pub mod dex {
                    pub mod trade {
                        include!(concat!(env!("OUT_DIR"), "/io.blockchain.v1.dex.trade.rs"));
                    }
                }
            }
        }
        pub mod chainstream {
            pub mod v1 {
                pub mod common {
                    include!(concat!(env!("OUT_DIR"), "/io.chainstream.v1.common.rs"));
                }
            }
        }
    }
}

use bs58;
use pb::io::blockchain::v1::dex::trade::{Trade, TradeEvent, TradeEvents};
use pb::io::chainstream::v1::common::{
    Block as CBlock, Chain, DApp as CDApp, Instruction as CInstruction, Status,
    Transaction as CTransaction,
};
use substreams_solana::pb::sf::solana::r#type::v1::Block;

const RAYDIUM: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

#[substreams::handlers::map]
fn map_ray_swap(block: Block) -> Result<TradeEvents, substreams::errors::Error> {
    let mut events: Vec<TradeEvent> = Vec::new();

    for tx in block.transactions() {
        let Some(meta) = tx.meta.as_ref() else {
            continue;
        };

        // 获取 tx 的 message 以解出 program_id
        let Some(message) = tx.transaction.as_ref().and_then(|t| t.message.as_ref()) else {
            continue;
        };

        // 是否包含 Raydium 指令
        let is_raydium = meta.inner_instructions.iter().any(|inner| {
            inner.instructions.iter().any(|ix| {
                let idx = ix.program_id_index as usize;
                message
                    .account_keys
                    .get(idx)
                    .map(|key| bs58::encode(key).into_string() == RAYDIUM)
                    .unwrap_or(false)
            })
        });

        if !is_raydium {
            continue;
        }

        // 检查是否有 Raydium swap 指令
        let has_raydium_swap = meta
            .log_messages
            .iter()
            .any(|log| log.contains("SwapRaydiumV4") || log.contains("Instruction: Swap"));

        if has_raydium_swap {
            // 打印完整信息
            substreams::log::info!("Full info: {:?}", meta.log_messages);
            substreams::log::info!("Full inner_instructions: {:?}", meta.inner_instructions);
            substreams::log::info!("Full post_balances: {:?}", meta.post_balances);
            substreams::log::info!("Full pre_balances: {:?}", meta.pre_balances);
            substreams::log::info!("Full post_token_balances: {:?}", meta.post_token_balances);
            substreams::log::info!("Full pre_token_balances: {:?}", meta.pre_token_balances);
            substreams::log::info!("Full meta: {:?}", meta.meta());

            // 遍历内层指令，限定到 Raydium 程序指令范围内，并准备后续所需变量
            for inner in &meta.inner_instructions {
                for (j, ix) in inner.instructions.iter().enumerate() {
                    let idx = ix.program_id_index as usize;
                    if let Some(key) = message.account_keys.get(idx) {
                        if bs58::encode(key).into_string() != RAYDIUM {
                            continue;
                        }

                        // 相关账户（按指令账户索引展开）
                        let accounts: Vec<String> = ix
                            .accounts
                            .iter()
                            .filter_map(|&acc_idx| message.account_keys.get(acc_idx as usize))
                            .map(|key| bs58::encode(key).into_string())
                            .collect();
                        substreams::log::info!("Raydium Swap Accounts: {:?}", accounts);

                        // 推断池地址（常见布局下 index 2 为池/状态账户），若缺失则回退为程序地址
                        let pool_address = accounts
                            .get(2)
                            .cloned()
                            .unwrap_or_else(|| RAYDIUM.to_string());

                        // 交易签名
                        let tx_signature = tx
                            .transaction
                            .as_ref()
                            .and_then(|t| t.signatures.get(0))
                            .map(|sig| bs58::encode(sig).into_string())
                            .unwrap_or_default();

                        // 便捷引用余额数组
                        let pre_balances = &meta.pre_balances;
                        let post_balances = &meta.post_balances;

                        // 计算多个账户的余额变化来获取交易数量
                        let mut amount_in = String::new();
                        let mut amount_out = String::new();
                        let mut user_pre_amount = String::new();
                        let mut user_post_amount = String::new();
                        let mut vault_a_pre_amount = String::new();
                        let mut vault_a_post_amount = String::new();
                        let mut vault_b_pre_amount = String::new();
                        let mut vault_b_post_amount = String::new();

                        // 检查前几个账户的余额变化
                        for i in 0..std::cmp::min(
                            accounts.len(),
                            pre_balances.len().min(post_balances.len()),
                        ) {
                            if let (Some(&pre), Some(&post)) =
                                (pre_balances.get(i), post_balances.get(i))
                            {
                                if pre != post {
                                    let change = if post > pre { post - pre } else { pre - post };

                                    if change > 0 {
                                        if amount_in.is_empty() {
                                            amount_in = change.to_string();
                                            user_pre_amount = pre.to_string();
                                            user_post_amount = post.to_string();
                                        } else if amount_out.is_empty()
                                            && change.to_string() != amount_in
                                        {
                                            amount_out = change.to_string();
                                            if vault_a_pre_amount.is_empty() {
                                                vault_a_pre_amount = pre.to_string();
                                                vault_a_post_amount = post.to_string();
                                            } else {
                                                vault_b_pre_amount = pre.to_string();
                                                vault_b_post_amount = post.to_string();
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 如果还没有找到amount_out，使用amount_in的一个估算值
                        if amount_out.is_empty() && !amount_in.is_empty() {
                            // 通常swap会有汇率，这里做一个简单估算
                            if let Ok(amount_val) = amount_in.parse::<u64>() {
                                amount_out = (amount_val / 1000).to_string(); // 简单的汇率估算
                            }
                        }

                        substreams::log::info!(
                            "Instruction data length: {}, Amount in: {}, Amount out: {}",
                            ix.data.len(),
                            amount_in,
                            amount_out
                        );

                        // 构造通用的 Instruction/Block/Transaction/DApp 以匹配 proto 定义
                        let instruction = CInstruction {
                            index: inner.index as u32,
                            is_inner_instruction: true,
                            inner_instruction_index: j as u32,
                            r#type: "RaydiumSwap".to_string(),
                        };

                        // Block 信息（尽力从 Solana Block 中映射；缺失字段使用默认值）
                        let c_block = CBlock {
                            timestamp: block
                                .block_time
                                .as_ref()
                                .map(|t| t.timestamp)
                                .unwrap_or_default(), // 如需时间戳，可从 block.block_time 提取
                            hash: block.blockhash.clone(), // 如需哈希，可从 block.blockhash 提取
                            height: block
                                .block_height
                                .as_ref()
                                .map(|h| h.block_height)
                                .unwrap_or_default(),
                            slot: block.slot, // 如需 slot，可从 block.slot 提取
                        };

                        // 获取费支付者/签名者（通常为第一个账户）
                        let fee_payer = message
                            .account_keys
                            .get(0)
                            .map(|k| bs58::encode(k).into_string())
                            .unwrap_or_default();

                        // 交易信息
                        let c_tx = CTransaction {
                            fee: meta.fee as u64,
                            fee_payer: fee_payer.clone(),
                            index: 0, // 如能获取 tx 索引可替换
                            signature: tx_signature.clone(),
                            signer: fee_payer,
                            status: if meta.err.is_none() {
                                Status::Success as i32
                            } else {
                                Status::Failed as i32
                            },
                        };

                        // DApp 信息
                        let d_app = CDApp {
                            program_address: RAYDIUM.to_string(),
                            inner_program_address: RAYDIUM.to_string(),
                            chain: Chain::Solana as i32,
                        };

                        let token_a_mint = meta
                            .pre_token_balances
                            .get(0)
                            .map(|b| b.mint.clone())
                            .unwrap_or_default();
                        let token_b_mint = meta
                            .pre_token_balances
                            .get(1)
                            .map(|b| b.mint.clone())
                            .unwrap_or_default();
                        // 依据 pre/post token balances 的变化区分用户卖出(A)与买入(B)侧
                        let account_pubkey = |idx: usize| {
                            message
                                .account_keys
                                .get(idx)
                                .map(|k| bs58::encode(k).into_string())
                                .unwrap_or_default()
                        };

                        use std::collections::HashMap;
                        let mut pre_map: HashMap<
                            u32,
                            &substreams_solana::pb::sf::solana::r#type::v1::TokenBalance,
                        > = HashMap::new();
                        for b in &meta.pre_token_balances {
                            pre_map.insert(b.account_index, b);
                        }
                        let mut post_map: HashMap<
                            u32,
                            &substreams_solana::pb::sf::solana::r#type::v1::TokenBalance,
                        > = HashMap::new();
                        for b in &meta.post_token_balances {
                            post_map.insert(b.account_index, b);
                        }

                        // 仅保留用户侧（owner != pool_address），计算 delta = post - pre
                        let mut user_side: Vec<(u32, i128, String)> = Vec::new();
                        for (&acc_idx, pre_b) in &pre_map {
                            if pre_b.owner == pool_address {
                                continue;
                            }
                            if let Some(post_b) = post_map.get(&acc_idx) {
                                let pre_amt: i128 = pre_b
                                    .ui_token_amount
                                    .as_ref()
                                    .and_then(|u| u.amount.parse::<i128>().ok())
                                    .unwrap_or(0);
                                let post_amt: i128 = post_b
                                    .ui_token_amount
                                    .as_ref()
                                    .and_then(|u| u.amount.parse::<i128>().ok())
                                    .unwrap_or(0);
                                let delta = post_amt - pre_amt; // 增加为正，减少为负
                                user_side.push((acc_idx, delta, pre_b.owner.clone()));
                            }
                        }

                        let mut user_a_token_account_address = String::new();
                        let mut user_a_account_owner_address = String::new();
                        let mut user_b_token_account_address = String::new();
                        let mut user_b_account_owner_address = String::new();

                        if let Some((acc_idx, _d, owner)) = user_side
                            .iter()
                            .min_by_key(|(_, d, _)| *d)
                            .map(|(a, b, c)| (*a, *b, c.clone()))
                        {
                            user_a_token_account_address = account_pubkey(acc_idx as usize);
                            user_a_account_owner_address = owner;
                        }
                        if let Some((acc_idx, _d, owner)) = user_side
                            .iter()
                            .max_by_key(|(_, d, _)| *d)
                            .map(|(a, b, c)| (*a, *b, c.clone()))
                        {
                            user_b_token_account_address = account_pubkey(acc_idx as usize);
                            user_b_account_owner_address = owner;
                        }

                        if user_b_token_account_address.is_empty() {
                            user_b_token_account_address = user_a_token_account_address.clone();
                            user_b_account_owner_address = user_a_account_owner_address.clone();
                        }

                        // 计算 A/B 侧的变动数量与 pre/post 数量（以原始 amount 计，字符串）
                        let mut user_a_amount_s = String::new();
                        let mut user_b_amount_s = String::new();
                        let mut user_a_pre_amount_s = String::new();
                        let mut user_a_post_amount_s = String::new();
                        let mut user_b_pre_amount_s = String::new();
                        let mut user_b_post_amount_s = String::new();

                        // 辅助：从 map 中取指定账户的字符串 amount（若无则为 "0"）
                        let get_amount_str = |m: &HashMap<
                            u32,
                            &substreams_solana::pb::sf::solana::r#type::v1::TokenBalance,
                        >,
                                              idx: u32|
                         -> String {
                            m.get(&idx)
                                .and_then(|b| b.ui_token_amount.as_ref())
                                .map(|u| u.amount.clone())
                                .unwrap_or_else(|| "0".to_string())
                        };

                        // 找到对应的 acc_idx 值
                        let user_a_idx_opt = user_side
                            .iter()
                            .min_by_key(|(_, d, _)| *d)
                            .map(|(idx, _, _)| *idx);
                        let user_b_idx_opt = user_side
                            .iter()
                            .max_by_key(|(_, d, _)| *d)
                            .map(|(idx, _, _)| *idx);

                        if let Some(a_idx) = user_a_idx_opt {
                            let pre = pre_map
                                .get(&a_idx)
                                .and_then(|b| b.ui_token_amount.as_ref())
                                .and_then(|u| u.amount.parse::<i128>().ok())
                                .unwrap_or(0);
                            let post = post_map
                                .get(&a_idx)
                                .and_then(|b| b.ui_token_amount.as_ref())
                                .and_then(|u| u.amount.parse::<i128>().ok())
                                .unwrap_or(0);
                            let delta = post - pre; // 负数为卖出
                            user_a_amount_s = delta.abs().to_string();
                            user_a_pre_amount_s = get_amount_str(&pre_map, a_idx);
                            user_a_post_amount_s = get_amount_str(&post_map, a_idx);
                        }

                        if let Some(b_idx) = user_b_idx_opt {
                            let pre = pre_map
                                .get(&b_idx)
                                .and_then(|b| b.ui_token_amount.as_ref())
                                .and_then(|u| u.amount.parse::<i128>().ok())
                                .unwrap_or(0);
                            let post = post_map
                                .get(&b_idx)
                                .and_then(|b| b.ui_token_amount.as_ref())
                                .and_then(|u| u.amount.parse::<i128>().ok())
                                .unwrap_or(0);
                            let delta = post - pre; // 正数为买入
                            user_b_amount_s = delta.abs().to_string();
                            user_b_pre_amount_s = get_amount_str(&pre_map, b_idx);
                            user_b_post_amount_s = get_amount_str(&post_map, b_idx);
                        }

                        // 判断方向：用户卖出侧(user_a)的 mint 是否等于 token_a_mint
                        let get_mint_by_idx = |idx: u32| -> String {
                            pre_map
                                .get(&idx)
                                .map(|b| b.mint.clone())
                                .or_else(|| post_map.get(&idx).map(|b| b.mint.clone()))
                                .unwrap_or_default()
                        };
                        let mut was_original_direction = true;
                        if let Some(a_idx) = user_a_idx_opt {
                            let user_a_mint = get_mint_by_idx(a_idx);
                            was_original_direction = (user_a_mint == token_a_mint);
                        }

                        // 计算池子金库（vault）账户：基于 owner == pool_address 且 mint 匹配 token_a/token_b
                        let find_pool_vault_idx = |mint: &str| -> Option<u32> {
                            // 优先从 pre_map 查找
                            let from_pre = pre_map.iter().find_map(|(&idx, b)| {
                                if b.owner == pool_address && b.mint == mint {
                                    Some(idx)
                                } else {
                                    None
                                }
                            });
                            if from_pre.is_some() {
                                return from_pre;
                            }
                            // 其次从 post_map 查找
                            post_map.iter().find_map(|(&idx, b)| {
                                if b.owner == pool_address && b.mint == mint {
                                    Some(idx)
                                } else {
                                    None
                                }
                            })
                        };

                        let vault_a_idx_opt = find_pool_vault_idx(&token_a_mint);
                        let vault_b_idx_opt = find_pool_vault_idx(&token_b_mint);

                        // 计算 vault 的地址、owner、变动与 pre/post 数量
                        let mut vault_a_address_s = String::new();
                        let mut vault_b_address_s = String::new();
                        let mut vault_a_owner_s = pool_address.clone();
                        let mut vault_b_owner_s = pool_address.clone();
                        let mut vault_a_amount_s = String::new();
                        let mut vault_b_amount_s = String::new();
                        let mut vault_a_pre_amount_s = String::new();
                        let mut vault_a_post_amount_s = String::new();
                        let mut vault_b_pre_amount_s = String::new();
                        let mut vault_b_post_amount_s = String::new();

                        if let Some(a_idx) = vault_a_idx_opt {
                            vault_a_address_s = account_pubkey(a_idx as usize);
                            vault_a_owner_s = pre_map
                                .get(&a_idx)
                                .map(|b| b.owner.clone())
                                .or_else(|| post_map.get(&a_idx).map(|b| b.owner.clone()))
                                .unwrap_or(pool_address.clone());
                            let pre = pre_map
                                .get(&a_idx)
                                .and_then(|b| b.ui_token_amount.as_ref())
                                .and_then(|u| u.amount.parse::<i128>().ok())
                                .unwrap_or(0);
                            let post = post_map
                                .get(&a_idx)
                                .and_then(|b| b.ui_token_amount.as_ref())
                                .and_then(|u| u.amount.parse::<i128>().ok())
                                .unwrap_or(0);
                            let delta = post - pre;
                            vault_a_amount_s = delta.abs().to_string();
                            vault_a_pre_amount_s = get_amount_str(&pre_map, a_idx);
                            vault_a_post_amount_s = get_amount_str(&post_map, a_idx);
                        }

                        if let Some(b_idx) = vault_b_idx_opt {
                            vault_b_address_s = account_pubkey(b_idx as usize);
                            vault_b_owner_s = pre_map
                                .get(&b_idx)
                                .map(|b| b.owner.clone())
                                .or_else(|| post_map.get(&b_idx).map(|b| b.owner.clone()))
                                .unwrap_or(pool_address.clone());
                            let pre = pre_map
                                .get(&b_idx)
                                .and_then(|b| b.ui_token_amount.as_ref())
                                .and_then(|u| u.amount.parse::<i128>().ok())
                                .unwrap_or(0);
                            let post = post_map
                                .get(&b_idx)
                                .and_then(|b| b.ui_token_amount.as_ref())
                                .and_then(|u| u.amount.parse::<i128>().ok())
                                .unwrap_or(0);
                            let delta = post - pre;
                            vault_b_amount_s = delta.abs().to_string();
                            vault_b_pre_amount_s = get_amount_str(&pre_map, b_idx);
                            vault_b_post_amount_s = get_amount_str(&post_map, b_idx);
                        }

                        let trade = Trade {
                            token_a_address: token_a_mint,
                            token_b_address: token_b_mint,
                            user_a_token_account_address: user_a_token_account_address,
                            user_a_account_owner_address: user_a_account_owner_address,
                            user_b_token_account_address: user_b_token_account_address,
                            user_b_account_owner_address: user_b_account_owner_address,
                            user_a_amount: user_a_amount_s,
                            user_b_amount: user_b_amount_s,
                            user_a_pre_amount: user_a_pre_amount_s,
                            user_a_post_amount: user_a_post_amount_s,
                            user_b_pre_amount: user_b_pre_amount_s,
                            user_b_post_amount: user_b_post_amount_s,
                            was_original_direction,
                            pool_address: pool_address.clone(),
                            vault_a: vault_a_address_s,
                            vault_b: vault_b_address_s,
                            vault_a_owner_address: vault_a_owner_s,
                            vault_b_owner_address: vault_b_owner_s,
                            vault_a_amount: vault_a_amount_s,
                            vault_b_amount: vault_b_amount_s,
                            vault_a_pre_amount: vault_a_pre_amount_s,
                            vault_b_pre_amount: vault_b_pre_amount_s,
                            vault_a_post_amount: vault_a_post_amount_s,
                            vault_b_post_amount: vault_b_post_amount_s,
                            pool_config_address: pool_address,
                        };

                        events.push(TradeEvent {
                            instruction: Some(instruction),
                            block: Some(c_block),
                            transaction: Some(c_tx),
                            d_app: Some(d_app),
                            trade: Some(trade),
                            bonding_curve: None,
                        });
                    }
                }
            }
        }
    }

    Ok(TradeEvents { events })
}
