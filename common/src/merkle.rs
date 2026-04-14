use crate::hash::{hash_account, hash_pair};
use crate::types::{L2Account, MerkleProof};

/// Capacity: 16 leaves (next power of 2 above 10 accounts)
const CAPACITY: usize = 16;
const DEPTH: usize = 4; // log2(16)

pub struct MerkleTree {
    /// Leaf hashes, length == CAPACITY, padded with [0u8;32]
    leaves: [[u8; 32]; CAPACITY],
    /// Full binary tree stored flat, length == 2*CAPACITY
    /// nodes[1] = root, nodes[2..3] = depth-1, nodes[CAPACITY..2*CAPACITY-1] = leaves
    nodes: [[u8; 32]; 2 * CAPACITY],
}

impl MerkleTree {
    /// Build tree from accounts sorted by pubkey. Accounts beyond CAPACITY are ignored.
    pub fn from_accounts(accounts: &[L2Account]) -> Self {
        // Sort by pubkey for determinism
        let mut sorted = accounts.to_vec();
        sorted.sort_by_key(|a| a.pubkey);

        let mut leaves = [[0u8; 32]; CAPACITY];
        for (i, acct) in sorted.iter().take(CAPACITY).enumerate() {
            leaves[i] = hash_account(acct);
        }

        let mut nodes = [[0u8; 32]; 2 * CAPACITY];
        // Copy leaves into bottom of tree (indices CAPACITY..2*CAPACITY)
        for (i, leaf) in leaves.iter().enumerate() {
            nodes[CAPACITY + i] = *leaf;
        }
        // Build internal nodes bottom-up
        for i in (1..CAPACITY).rev() {
            nodes[i] = hash_pair(&nodes[2 * i], &nodes[2 * i + 1]);
        }

        MerkleTree { leaves, nodes }
    }

    /// Root hash of the tree
    pub fn root(&self) -> [u8; 32] {
        self.nodes[1]
    }

    /// Generate a merkle proof for the leaf at `index` (0-based among sorted leaves)
    pub fn proof(&self, index: usize) -> MerkleProof {
        assert!(index < CAPACITY);
        let leaf = self.leaves[index];
        let mut siblings = Vec::with_capacity(DEPTH);
        let mut node_idx = CAPACITY + index;

        for _ in 0..DEPTH {
            let sibling_idx = if node_idx % 2 == 0 {
                node_idx + 1
            } else {
                node_idx - 1
            };
            siblings.push(self.nodes[sibling_idx]);
            node_idx /= 2;
        }

        MerkleProof {
            leaf,
            siblings,
            index: index as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_account(seed: u8, lamports: u64) -> L2Account {
        L2Account {
            pubkey: [seed; 32],
            lamports,
            nonce: 0,
        }
    }

    #[test]
    fn build_and_verify_all_proofs() {
        let accounts: Vec<L2Account> = (0..10).map(|i| make_account(i, 1000 * i as u64)).collect();
        let tree = MerkleTree::from_accounts(&accounts);
        let root = tree.root();

        for i in 0..10 {
            let proof = tree.proof(i);
            assert!(proof.verify(&root), "proof {} should verify", i);
        }
    }

    #[test]
    fn modified_account_changes_root() {
        let accounts: Vec<L2Account> = (0..10).map(|i| make_account(i, 1000)).collect();
        let tree1 = MerkleTree::from_accounts(&accounts);

        let mut modified = accounts.clone();
        modified[5].lamports = 9999;
        let tree2 = MerkleTree::from_accounts(&modified);

        assert_ne!(tree1.root(), tree2.root());
    }

    #[test]
    fn tampered_proof_fails() {
        let accounts: Vec<L2Account> = (0..10).map(|i| make_account(i, 1000)).collect();
        let tree = MerkleTree::from_accounts(&accounts);
        let root = tree.root();

        let mut proof = tree.proof(3);
        proof.siblings[0] = [0xff; 32]; // corrupt one sibling
        assert!(!proof.verify(&root));
    }

    #[test]
    fn wrong_index_proof_fails() {
        let accounts: Vec<L2Account> = (0..10).map(|i| make_account(i, 1000)).collect();
        let tree = MerkleTree::from_accounts(&accounts);
        let root = tree.root();

        let mut proof = tree.proof(2);
        proof.index = 7; // wrong path
        assert!(!proof.verify(&root));
    }

    #[test]
    fn empty_slots_are_zero_hashes() {
        // 10 accounts -> leaves 10..15 should be [0;32]
        let accounts: Vec<L2Account> = (0..10).map(|i| make_account(i, 1000)).collect();
        let tree = MerkleTree::from_accounts(&accounts);
        for i in 10..CAPACITY {
            assert_eq!(tree.leaves[i], [0u8; 32]);
        }
    }
}
