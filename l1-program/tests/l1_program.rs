use litesvm::LiteSVM;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction, InstructionError},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_program,
    transaction::{Transaction, TransactionError},
};

// ── Constants (must match l1_program.s) ───────────────────────────────────────

const STATE_SZ: usize = 0x55; // 85 bytes

const STATE_INITIALIZED: usize = 0x00;
const STATE_SEQ_PUBKEY: usize = 0x01;
const STATE_ROOT: usize = 0x21;
const STATE_BATCH_NUM: usize = 0x41;
const STATE_VAULT_BUMP: usize = 0x52;
const STATE_WITHDRAW_MASK: usize = 0x53;

// ── Error codes (must match l1_program.s) ─────────────────────────────────────

const ERR_INVALID_IX: u32 = 0x1;
const ERR_NOT_INITIALIZED: u32 = 0x6;
const ERR_ALREADY_INITIALIZED: u32 = 0x7;
const ERR_WRONG_SEQUENCER: u32 = 0x8;
const ERR_BAD_BATCH_NUM: u32 = 0x9;
const ERR_INSUFFICIENT_VAULT: u32 = 0xA;
const ERR_BAD_PROOF: u32 = 0xB;
const ERR_ALREADY_WITHDRAWN: u32 = 0xC;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (LiteSVM, Pubkey) {
    let mut svm = LiteSVM::new();
    let program_id_bytes: [u8; 32] =
        std::fs::read("deploy/l1-program-keypair.json").unwrap()[..32]
            .try_into()
            .unwrap();
    let program_id = Pubkey::from(program_id_bytes);
    svm.add_program_from_file(program_id, "deploy/l1_program.so")
        .expect("failed to load l1-program.so — run cargo build-sbf first");
    (svm, program_id)
}

fn state_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"state"], program_id)
}

fn vault_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault"], program_id)
}

fn send(
    svm: &mut LiteSVM,
    ix: Instruction,
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        signers,
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx)
}

fn custom_err(code: u32) -> TransactionError {
    TransactionError::InstructionError(0, InstructionError::Custom(code))
}

fn print_logs(
    label: &str,
    result: &Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata>,
) {
    let logs = match result {
        Ok(m) => &m.logs,
        Err(e) => &e.meta.logs,
    };
    println!("[{}]", label);
    for log in logs {
        println!("  {}", log);
    }
}

// ── Tests (stubs — filled in as each instruction is implemented) ──────────────

#[test]
fn test_invalid_discriminator() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    let ix = Instruction::new_with_bytes(
        program_id,
        &[0xFF], // unknown disc
        vec![],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("invalid_discriminator", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_INVALID_IX));
}
