use sha2::{Digest, Sha256};

use crate::types::L2Account;

/// Hash a single L2 account into a 32-byte leaf
pub fn hash_account(account: &L2Account) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&account.pubkey);
    hasher.update(account.lamports.to_le_bytes());
    hasher.update(account.nonce.to_le_bytes());
    hasher.finalize().into()
}

/// Hash two 32-byte nodes into a parent node (left || right)
pub fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// Hash arbitrary bytes
pub fn hash_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}
