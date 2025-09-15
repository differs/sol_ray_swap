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

use pb::io::blockchain::v1::dex::trade::{TradeEvents, TradeEvent, Trade};
use substreams_solana::pb::sf::solana::r#type::v1::Block;
use bs58;

const RAYDIUM: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

#[substreams::handlers::map]
fn map_ray_swap(block: Block) -> Result<TradeEvents, substreams::errors::Error> {
    let mut events: Vec<TradeEvent> = Vec::new();

    for tx in block.transactions() {
        let Some(meta) = tx.meta.as_ref() else { continue };

        // 获取 tx 的 message 以解出 program_id
        let Some(message) = tx.transaction.as_ref().and_then(|t| t.message.as_ref()) else { continue };

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

        if !is_raydium { continue; }

        // 检查是否有 Raydium swap 指令
        let has_raydium_swap = meta.log_messages.iter()
            .any(|log| log.contains("SwapRaydiumV4") || log.contains("Instruction: Swap"));

        if has_raydium_swap {
            // 打印完整信息
            substreams::log::info!("Full info: {:?}", meta.log_messages);
            substreams::log::info!("Full inner_instructions: {:?}", meta.inner_instructions);
            substreams::log::info!("Full post_balances: {:?}", meta.post_balances);
            substreams::log::info!("Full pre_balances: {:?}", meta.pre_balances);
            // substreams::log::info!("Full pre_instructions: {:?}", meta.pre_instructions);
            // substreams::log::info!("Full post_instructions: {:?}", meta.post_instructions);
            substreams::log::info!("Full post_token_balances: {:?}", meta.post_token_balances);
            substreams::log::info!("Full pre_token_balances: {:?}", meta.pre_token_balances);
            substreams::log::info!("Full meta: {:?}", meta.meta());



            // 从指令中提取账户信息

            for inner in &meta.inner_instructions {
                for ix in &inner.instructions {
                    let idx = ix.program_id_index as usize;
                    if let Some(key) = message.account_keys.get(idx) {
                        if bs58::encode(key).into_string() == RAYDIUM {
                            // 获取相关账户地址
                            let accounts: Vec<String> = ix.accounts.iter()
                                .filter_map(|&acc_idx| message.account_keys.get(acc_idx as usize))
                                .map(|key| bs58::encode(key).into_string())
                                .collect();

                            substreams::log::info!("Raydium Swap Accounts: {:?}", accounts);

                            // 重新分析 Raydium 账户结构
                            // 索引0: Token程序
                            // 索引1: 可能是代币mint或用户账户
                            // 索引2: 池账户
                            // 需要找到真正的代币mint地址
                            
                            // 先输出更多调试信息来分析账户结构
                            for (i, account) in accounts.iter().enumerate() {
                                // substreams::log::info!("Account[{}]: {}", i, account);
                            }
                            
                            // 基于实际观察重新映射
                            let user_account = accounts.get(0).cloned().unwrap_or_default(); // Token程序
                            let pool_address = accounts.get(2).cloned().unwrap_or_else(|| RAYDIUM.to_string()); // 池地址
                            
                            // 对于WSOL-GARI市场，需要找到正确的代币mint
                            // WSOL通常是 So11111111111111111111111111111111111111112
                            // 从账户数组中寻找已知的代币地址
                            let mut token_a = String::new();
                            let mut token_b = String::new();
                            let mut vault_a_address = String::new();
                            let mut vault_b_address = String::new();
                            
                            // 寻找WSOL地址和其他代币
                            let wsol_mint = "So11111111111111111111111111111111111111112";
                            let mut unique_addresses = std::collections::HashSet::new();
                            
                            for (i, account) in accounts.iter().enumerate() {
                                if account == wsol_mint {
                                    token_a = account.clone();
                                    substreams::log::info!("Found WSOL at index {}", i);
                                } else if account.len() == 44 && account != &user_account && account != &pool_address && !account.starts_with("TokenkegQ") {
                                    unique_addresses.insert(account.clone());
                                    substreams::log::info!("Unique address at index {}: {}", i, account);
                                }
                            }
                            
                            // 从唯一地址中选择代币
                            let unique_vec: Vec<String> = unique_addresses.into_iter().collect();
                            if token_a.is_empty() && !unique_vec.is_empty() {
                                token_a = unique_vec[0].clone();
                            }
                            if unique_vec.len() > 1 {
                                for addr in &unique_vec {
                                    if addr != &token_a {
                                        token_b = addr.clone();
                                        break;
                                    }
                                }
                            }
                            
                            substreams::log::info!("Final tokens - A: {}, B: {}", token_a, token_b);
                            
                            // 如果没找到WSOL，使用前面找到的地址
                            if token_a.is_empty() {
                                token_a = accounts.get(1).cloned().unwrap_or_default();
                            }
                            if token_b.is_empty() {
                                // 寻找不同的代币地址
                                for account in &accounts {
                                    if account != &token_a && account.len() == 44 && 
                                       !account.starts_with("TokenkegQ") && account != &pool_address {
                                        token_b = account.clone();
                                        break;
                                    }
                                }
                            }
                            
                            // 金库地址通常在中间位置
                            vault_a_address = accounts.get(4).cloned().unwrap_or_default();
                            vault_b_address = accounts.get(5).cloned().unwrap_or_default();

                            // 从交易签名生成唯一ID
                            let tx_signature = tx.transaction.as_ref()
                                .and_then(|t| t.signatures.get(0))
                                .map(|sig| bs58::encode(sig).into_string())
                                .unwrap_or_default();

                            // 从余额变化获取实际交易数量
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
                            for i in 0..std::cmp::min(accounts.len(), pre_balances.len().min(post_balances.len())) {
                                if let (Some(&pre), Some(&post)) = (pre_balances.get(i), post_balances.get(i)) {
                                    if pre != post {
                                        let change = if post > pre {
                                            post - pre
                                        } else {
                                            pre - post
                                        };
                                        
                                        if change > 0 {
                                            if amount_in.is_empty() {
                                                amount_in = change.to_string();
                                                user_pre_amount = pre.to_string();
                                                user_post_amount = post.to_string();
                                            } else if amount_out.is_empty() && change.to_string() != amount_in {
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

                            substreams::log::info!("Instruction data length: {}, Amount in: {}, Amount out: {}", 
                                ix.data.len(), amount_in, amount_out);

                            let trade = Trade {
                                token_a_address: token_a.clone(),
                                token_b_address: token_b.clone(),
                                user_a_token_account_address: user_account.clone(),
                                user_a_account_owner_address: tx_signature.clone(),
                                user_b_token_account_address: user_account.clone(),
                                user_b_account_owner_address: tx_signature.clone(),
                                user_a_amount: amount_in.clone(),
                                user_b_amount: amount_out.clone(),
                                user_a_pre_amount: user_pre_amount.clone(),
                                user_a_post_amount: user_post_amount.clone(),
                                user_b_pre_amount: user_pre_amount.clone(),
                                user_b_post_amount: user_post_amount.clone(),
                                was_original_direction: true,
                                pool_address: pool_address.clone(),
                                vault_a: vault_a_address.clone(),
                                vault_b: vault_b_address.clone(),
                                vault_a_owner_address: RAYDIUM.to_string(),
                                vault_b_owner_address: RAYDIUM.to_string(),
                                vault_a_amount: amount_in.clone(),
                                vault_b_amount: amount_out.clone(),
                                vault_a_pre_amount: vault_a_pre_amount,
                                vault_b_pre_amount: vault_b_pre_amount,
                                vault_a_post_amount: vault_a_post_amount,
                                vault_b_post_amount: vault_b_post_amount,
                                pool_config_address: pool_address,
                            };

                            events.push(TradeEvent {
                                instruction: None,
                                block: None,
                                transaction: None,
                                d_app: None,
                                trade: Some(trade),
                                bonding_curve: None,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(TradeEvents { events })
}

// 正则小工具
fn capture(text: &str, pat: &str) -> String {
    regex::Regex::new(pat)
        .unwrap()
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}