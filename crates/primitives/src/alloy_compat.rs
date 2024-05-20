//! Common conversions from alloy types.

use crate::{
    Block, Header, Transaction, TransactionSigned, TxEip1559, TxEip2930, TxEip4844, TxLegacy,
    TxType, U256,
};
use alloy_primitives::TxKind;
use alloy_rlp::Error as RlpError;
use alloy_rpc_types::Parity;

impl TryFrom<alloy_rpc_types::Block> for Block {
    type Error = alloy_rpc_types::ConversionError;

    fn try_from(block: alloy_rpc_types::Block) -> Result<Self, Self::Error> {
        use alloy_rpc_types::ConversionError;

        let body = {
            let transactions: Result<Vec<TransactionSigned>, ConversionError> = match block
                .transactions
            {
                alloy_rpc_types::BlockTransactions::Full(transactions) => transactions
                    .into_iter()
                    .map(|tx| {
                        let signature = tx.signature.ok_or(ConversionError::MissingSignature)?;
                        let recovery_id = if signature.v > U256::from(1) {
                            signature.v - U256::from(37)
                        } else {
                            signature.v
                        };
                        Ok(TransactionSigned::from_transaction_and_signature(
                            tx.try_into()?,
                            crate::Signature {
                                r: signature.r,
                                s: signature.s,
                                odd_y_parity: signature
                                    .y_parity
                                    .unwrap_or(Parity(recovery_id == U256::from(1)))
                                    .0,
                            },
                        ))
                    })
                    .collect(),
                alloy_rpc_types::BlockTransactions::Hashes(_) |
                alloy_rpc_types::BlockTransactions::Uncle => {
                    return Err(ConversionError::MissingFullTransactions)
                }
            };
            transactions?
        };

        Ok(Self {
            header: block.header.try_into()?,
            body,
            ommers: Default::default(),
            withdrawals: block.withdrawals.map(Into::into),
        })
    }
}

impl TryFrom<alloy_rpc_types::Header> for Header {
    type Error = alloy_rpc_types::ConversionError;

    fn try_from(header: alloy_rpc_types::Header) -> Result<Self, Self::Error> {
        use alloy_rpc_types::ConversionError;

        Ok(Self {
            base_fee_per_gas: header
                .base_fee_per_gas
                .map(|base_fee_per_gas| {
                    base_fee_per_gas.try_into().map_err(ConversionError::BaseFeePerGasConversion)
                })
                .transpose()?,
            beneficiary: header.miner,
            blob_gas_used: header
                .blob_gas_used
                .map(|blob_gas_used| {
                    blob_gas_used.try_into().map_err(ConversionError::BlobGasUsedConversion)
                })
                .transpose()?,
            difficulty: header.difficulty,
            excess_blob_gas: header
                .excess_blob_gas
                .map(|excess_blob_gas| {
                    excess_blob_gas.try_into().map_err(ConversionError::ExcessBlobGasConversion)
                })
                .transpose()?,
            extra_data: header.extra_data,
            gas_limit: header.gas_limit.try_into().map_err(ConversionError::GasLimitConversion)?,
            gas_used: header.gas_used.try_into().map_err(ConversionError::GasUsedConversion)?,
            logs_bloom: header.logs_bloom,
            mix_hash: header.mix_hash.unwrap_or_default(),
            nonce: u64::from_be_bytes(header.nonce.unwrap_or_default().0),
            number: header.number.ok_or(ConversionError::MissingBlockNumber)?,
            ommers_hash: header.uncles_hash,
            parent_beacon_block_root: header.parent_beacon_block_root,
            parent_hash: header.parent_hash,
            receipts_root: header.receipts_root,
            state_root: header.state_root,
            timestamp: header.timestamp,
            transactions_root: header.transactions_root,
            withdrawals_root: header.withdrawals_root,
        })
    }
}

impl TryFrom<alloy_rpc_types::Transaction> for Transaction {
    type Error = alloy_rpc_types::ConversionError;

    fn try_from(tx: alloy_rpc_types::Transaction) -> Result<Self, Self::Error> {
        use alloy_eips::eip2718::Eip2718Error;
        use alloy_rpc_types::ConversionError;

        match tx.transaction_type.map(TryInto::try_into).transpose().map_err(|_| {
            ConversionError::Eip2718Error(Eip2718Error::UnexpectedType(
                tx.transaction_type.unwrap(),
            ))
        })? {
            None | Some(TxType::Legacy) => {
                // legacy
                if tx.max_fee_per_gas.is_some() || tx.max_priority_fee_per_gas.is_some() {
                    return Err(ConversionError::Eip2718Error(
                        RlpError::Custom("EIP-1559 fields are present in a legacy transaction")
                            .into(),
                    ))
                }
                Ok(Transaction::Legacy(TxLegacy {
                    chain_id: tx.chain_id,
                    nonce: tx.nonce,
                    gas_price: tx.gas_price.ok_or(ConversionError::MissingGasPrice)?,
                    gas_limit: tx
                        .gas
                        .try_into()
                        .map_err(|_| ConversionError::Eip2718Error(RlpError::Overflow.into()))?,
                    to: tx.to.map_or(TxKind::Create, TxKind::Call),
                    value: tx.value,
                    input: tx.input,
                }))
            }
            Some(TxType::Eip2930) => {
                // eip2930
                Ok(Transaction::Eip2930(TxEip2930 {
                    chain_id: tx.chain_id.ok_or(ConversionError::MissingChainId)?,
                    nonce: tx.nonce,
                    gas_limit: tx
                        .gas
                        .try_into()
                        .map_err(|_| ConversionError::Eip2718Error(RlpError::Overflow.into()))?,
                    to: tx.to.map_or(TxKind::Create, TxKind::Call),
                    value: tx.value,
                    input: tx.input,
                    access_list: tx.access_list.ok_or(ConversionError::MissingAccessList)?,
                    gas_price: tx.gas_price.ok_or(ConversionError::MissingGasPrice)?,
                }))
            }
            Some(TxType::Eip1559) => {
                // EIP-1559
                Ok(Transaction::Eip1559(TxEip1559 {
                    chain_id: tx.chain_id.ok_or(ConversionError::MissingChainId)?,
                    nonce: tx.nonce,
                    max_priority_fee_per_gas: tx
                        .max_priority_fee_per_gas
                        .ok_or(ConversionError::MissingMaxPriorityFeePerGas)?,
                    max_fee_per_gas: tx
                        .max_fee_per_gas
                        .ok_or(ConversionError::MissingMaxFeePerGas)?,
                    gas_limit: tx
                        .gas
                        .try_into()
                        .map_err(|_| ConversionError::Eip2718Error(RlpError::Overflow.into()))?,
                    to: tx.to.map_or(TxKind::Create, TxKind::Call),
                    value: tx.value,
                    access_list: tx.access_list.ok_or(ConversionError::MissingAccessList)?,
                    input: tx.input,
                }))
            }
            Some(TxType::Eip4844) => {
                // EIP-4844
                Ok(Transaction::Eip4844(TxEip4844 {
                    chain_id: tx.chain_id.ok_or(ConversionError::MissingChainId)?,
                    nonce: tx.nonce,
                    max_priority_fee_per_gas: tx
                        .max_priority_fee_per_gas
                        .ok_or(ConversionError::MissingMaxPriorityFeePerGas)?,
                    max_fee_per_gas: tx
                        .max_fee_per_gas
                        .ok_or(ConversionError::MissingMaxFeePerGas)?,
                    gas_limit: tx
                        .gas
                        .try_into()
                        .map_err(|_| ConversionError::Eip2718Error(RlpError::Overflow.into()))?,
                    to: tx.to.map_or(TxKind::Create, TxKind::Call),
                    value: tx.value,
                    access_list: tx.access_list.ok_or(ConversionError::MissingAccessList)?,
                    input: tx.input,
                    blob_versioned_hashes: tx
                        .blob_versioned_hashes
                        .ok_or(ConversionError::MissingBlobVersionedHashes)?,
                    max_fee_per_blob_gas: tx
                        .max_fee_per_blob_gas
                        .ok_or(ConversionError::MissingMaxFeePerBlobGas)?,
                }))
            }
            #[cfg(feature = "optimism")]
            Some(TxType::Deposit) => todo!(),
        }
    }
}
