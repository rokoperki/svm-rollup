use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};

/// A single L2 account
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2Account {
    pub pubkey: [u8; 32],
    pub lamports: u64,
    pub nonce: u64,
}

/// A batch of executed transactions committed to L1
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateBatch {
    pub batch_number: u64,
    pub prev_state_root: [u8; 32],
    pub new_state_root: [u8; 32],
    pub tx_count: u32,
    #[serde_as(as = "Bytes")]
    pub sequencer_signature: [u8; 64],
}

/// An L2 transfer transaction
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct L2Transaction {
    pub from: [u8; 32],
    pub to: [u8; 32],
    pub amount: u64,
    pub nonce: u64,
    #[serde_as(as = "Bytes")]
    pub signature: [u8; 64],
}

/// Merkle inclusion proof for an L2 account
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleProof {
    /// The leaf hash being proven
    pub leaf: [u8; 32],
    /// Sibling hashes from leaf to root (depth 4 for 16 leaves)
    pub siblings: Vec<[u8; 32]>,
    /// Bit-path: bit i = 0 means leaf is on left at level i
    pub index: u32,
}

impl MerkleProof {
    /// Verify this proof against a known root
    pub fn verify(&self, root: &[u8; 32]) -> bool {
        let mut current = self.leaf;
        for (i, sibling) in self.siblings.iter().enumerate() {
            let bit = (self.index >> i) & 1;
            current = if bit == 0 {
                crate::hash::hash_pair(&current, sibling)
            } else {
                crate::hash::hash_pair(sibling, &current)
            };
        }
        &current == root
    }
}
