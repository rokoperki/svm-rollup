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
const ERR_WRONG_ACCT_COUNT: u32 = 0x2;
const ERR_NOT_SIGNER: u32 = 0x3;
const ERR_ALREADY_INITIALIZED: u32 = 0x7;
const ERR_NOT_INITIALIZED: u32 = 0x6;
const ERR_WRONG_SEQUENCER: u32 = 0x8;
const ERR_BAD_BATCH_NUM: u32 = 0x9;
const ERR_INSUFFICIENT_VAULT: u32 = 0xA;
const ERR_BAD_PROOF: u32 = 0xB;
const ERR_ALREADY_WITHDRAWN: u32 = 0xC;
const ERR_BAD_PDA: u32 = 0xD;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (LiteSVM, Pubkey) {
    let mut svm = LiteSVM::new();
    let program_id_bytes: [u8; 32] = std::fs::read("deploy/l1-program-keypair.json").unwrap()[..32]
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

// ── Initialize helpers ────────────────────────────────────────────────────────

fn make_init_ix_data(seq_pubkey: &Pubkey, state_bump: u8, vault_bump: u8) -> Vec<u8> {
    let mut data = vec![0x00u8]; // discriminator
    data.extend_from_slice(seq_pubkey.as_ref());
    data.push(state_bump);
    data.push(vault_bump);
    data
}

fn setup_pdas(svm: &mut LiteSVM, program_id: &Pubkey) -> (Pubkey, u8, Pubkey, u8) {
    let (state_key, state_bump) = state_pda(program_id);
    let (vault_key, vault_bump) = vault_pda(program_id);

    svm.set_account(
        state_key,
        Account {
            lamports: 1_141_440,
            data: vec![0u8; STATE_SZ],
            owner: *program_id,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    svm.set_account(
        vault_key,
        Account {
            lamports: 890_880,
            data: vec![],
            owner: *program_id,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    (state_key, state_bump, vault_key, vault_bump)
}

fn init_ix(
    program_id: Pubkey,
    payer: &Pubkey,
    state_key: Pubkey,
    vault_key: Pubkey,
    seq_pubkey: &Pubkey,
    state_bump: u8,
    vault_bump: u8,
) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &make_init_ix_data(seq_pubkey, state_bump, vault_bump),
        vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(state_key, false),
            AccountMeta::new(vault_key, false),
        ],
    )
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

#[test]
fn test_init_success() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let sequencer = Keypair::new();
    let (state_key, state_bump, vault_key, vault_bump) = setup_pdas(&mut svm, &program_id);

    let ix = init_ix(
        program_id,
        &payer.pubkey(),
        state_key,
        vault_key,
        &sequencer.pubkey(),
        state_bump,
        vault_bump,
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_success", &result);
    assert!(result.is_ok());

    let state_acct = svm.get_account(&state_key).unwrap();
    let d = &state_acct.data;
    assert_eq!(d[STATE_INITIALIZED], 1);
    assert_eq!(
        &d[STATE_SEQ_PUBKEY..STATE_SEQ_PUBKEY + 32],
        sequencer.pubkey().as_ref()
    );
    assert_eq!(&d[STATE_ROOT..STATE_ROOT + 32], &[0u8; 32]);
    assert_eq!(&d[STATE_BATCH_NUM..STATE_BATCH_NUM + 8], &[0u8; 8]);
    assert_eq!(d[STATE_VAULT_BUMP], vault_bump);
    assert_eq!(&d[STATE_WITHDRAW_MASK..STATE_WITHDRAW_MASK + 2], &[0u8; 2]);
}

#[test]
fn test_init_already_initialized() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let sequencer = Keypair::new();
    let (state_key, state_bump, vault_key, vault_bump) = setup_pdas(&mut svm, &program_id);

    let ix = init_ix(
        program_id,
        &payer.pubkey(),
        state_key,
        vault_key,
        &sequencer.pubkey(),
        state_bump,
        vault_bump,
    );
    send(&mut svm, ix, &payer, &[&payer]).unwrap();

    // rotate blockhash so the second tx has a different signature
    svm.expire_blockhash();

    // second call must fail
    let ix2 = init_ix(
        program_id,
        &payer.pubkey(),
        state_key,
        vault_key,
        &sequencer.pubkey(),
        state_bump,
        vault_bump,
    );
    let result = send(&mut svm, ix2, &payer, &[&payer]);
    print_logs("init_already_initialized", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_ALREADY_INITIALIZED));
}

#[test]
fn test_init_wrong_acct_count() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let sequencer = Keypair::new();
    let (state_key, state_bump, vault_key, vault_bump) = setup_pdas(&mut svm, &program_id);

    // only 2 accounts instead of 3
    let ix = Instruction::new_with_bytes(
        program_id,
        &make_init_ix_data(&sequencer.pubkey(), state_bump, vault_bump),
        vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(state_key, false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_wrong_acct_count", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_COUNT));
}

#[test]
fn test_init_not_signer() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let sequencer = Keypair::new();
    let authority = Keypair::new(); // acct0 — does NOT sign the tx
    let (state_key, state_bump, vault_key, vault_bump) = setup_pdas(&mut svm, &program_id);

    // authority is acct0 but is not a tx signer → is_signer=0 in the runtime
    let ix = Instruction::new_with_bytes(
        program_id,
        &make_init_ix_data(&sequencer.pubkey(), state_bump, vault_bump),
        vec![
            AccountMeta::new(authority.pubkey(), false), // not signer
            AccountMeta::new(state_key, false),
            AccountMeta::new(vault_key, false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_not_signer", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_SIGNER));
}

#[test]
fn test_init_bad_pda() {
    let (mut svm, program_id) = setup();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let sequencer = Keypair::new();
    let (_, state_bump, vault_key, vault_bump) = setup_pdas(&mut svm, &program_id);

    // wrong state key — random pubkey with STATE_SZ data
    let wrong_state = Keypair::new();
    svm.set_account(
        wrong_state.pubkey(),
        Account {
            lamports: 1_141_440,
            data: vec![0u8; STATE_SZ],
            owner: program_id,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let ix = init_ix(
        program_id,
        &payer.pubkey(),
        wrong_state.pubkey(),
        vault_key,
        &sequencer.pubkey(),
        state_bump,
        vault_bump,
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("init_bad_pda", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_BAD_PDA));
}

// ── UpdateStateRoot helpers ───────────────────────────────────────────────────

fn make_usr_ix_data(new_root: &[u8; 32], batch_num: u64) -> Vec<u8> {
    let mut data = vec![0x01u8]; // discriminator
    data.extend_from_slice(new_root);
    data.extend_from_slice(&batch_num.to_le_bytes());
    data
}

fn usr_ix(
    program_id: Pubkey,
    sequencer: &Pubkey,
    state_key: Pubkey,
    new_root: &[u8; 32],
    batch_num: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &make_usr_ix_data(new_root, batch_num),
        vec![
            AccountMeta::new(*sequencer, true),
            AccountMeta::new(state_key, false),
        ],
    )
}

fn setup_initialized(svm: &mut LiteSVM, program_id: &Pubkey, sequencer: &Pubkey) -> Pubkey {
    let (state_key, state_bump, vault_key, vault_bump) = setup_pdas(svm, program_id);
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let ix = init_ix(
        *program_id,
        &payer.pubkey(),
        state_key,
        vault_key,
        sequencer,
        state_bump,
        vault_bump,
    );
    send(svm, ix, &payer, &[&payer]).unwrap();
    state_key
}

// ── UpdateStateRoot tests ─────────────────────────────────────────────────────

#[test]
fn test_usr_success() {
    let (mut svm, program_id) = setup();
    let sequencer = Keypair::new();
    svm.airdrop(&sequencer.pubkey(), 1_000_000_000).unwrap();
    let state_key = setup_initialized(&mut svm, &program_id, &sequencer.pubkey());

    let new_root = [0xABu8; 32];
    let ix = usr_ix(program_id, &sequencer.pubkey(), state_key, &new_root, 1);
    let result = send(&mut svm, ix, &sequencer, &[&sequencer]);
    print_logs("usr_success", &result);
    assert!(result.is_ok());

    let d = svm.get_account(&state_key).unwrap().data;
    assert_eq!(&d[STATE_ROOT..STATE_ROOT + 32], &new_root);
    assert_eq!(
        &d[STATE_BATCH_NUM..STATE_BATCH_NUM + 8],
        &1u64.to_le_bytes()
    );
}

#[test]
fn test_usr_wrong_acct_count() {
    let (mut svm, program_id) = setup();
    let sequencer = Keypair::new();
    svm.airdrop(&sequencer.pubkey(), 1_000_000_000).unwrap();
    let state_key = setup_initialized(&mut svm, &program_id, &sequencer.pubkey());

    let ix = Instruction::new_with_bytes(
        program_id,
        &make_usr_ix_data(&[1u8; 32], 1),
        vec![AccountMeta::new(sequencer.pubkey(), true)], // only 1 account
    );
    let result = send(&mut svm, ix, &sequencer, &[&sequencer]);
    print_logs("usr_wrong_acct_count", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_ACCT_COUNT));
}

#[test]
fn test_usr_not_signer() {
    let (mut svm, program_id) = setup();
    let sequencer = Keypair::new();
    let payer = Keypair::new();
    svm.airdrop(&sequencer.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let state_key = setup_initialized(&mut svm, &program_id, &sequencer.pubkey());

    // sequencer is acct0 but does not sign — payer pays fees only
    let ix = Instruction::new_with_bytes(
        program_id,
        &make_usr_ix_data(&[1u8; 32], 1),
        vec![
            AccountMeta::new(sequencer.pubkey(), false), // not signer
            AccountMeta::new(state_key, false),
        ],
    );
    let result = send(&mut svm, ix, &payer, &[&payer]);
    print_logs("usr_not_signer", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_SIGNER));
}

#[test]
fn test_usr_not_initialized() {
    let (mut svm, program_id) = setup();
    let sequencer = Keypair::new();
    svm.airdrop(&sequencer.pubkey(), 1_000_000_000).unwrap();
    let (state_key, _, _, _) = setup_pdas(&mut svm, &program_id); // PDAs exist but not initialized

    let ix = usr_ix(program_id, &sequencer.pubkey(), state_key, &[1u8; 32], 1);
    let result = send(&mut svm, ix, &sequencer, &[&sequencer]);
    print_logs("usr_not_initialized", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_NOT_INITIALIZED));
}

#[test]
fn test_usr_wrong_sequencer() {
    let (mut svm, program_id) = setup();
    let sequencer = Keypair::new();
    let impostor = Keypair::new();
    svm.airdrop(&sequencer.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&impostor.pubkey(), 1_000_000_000).unwrap();
    let state_key = setup_initialized(&mut svm, &program_id, &sequencer.pubkey());

    // impostor signs but state has sequencer's key
    let ix = usr_ix(program_id, &impostor.pubkey(), state_key, &[1u8; 32], 1);
    let result = send(&mut svm, ix, &impostor, &[&impostor]);
    print_logs("usr_wrong_sequencer", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_WRONG_SEQUENCER));
}

#[test]
fn test_usr_bad_batch_num() {
    let (mut svm, program_id) = setup();
    let sequencer = Keypair::new();
    svm.airdrop(&sequencer.pubkey(), 1_000_000_000).unwrap();
    let state_key = setup_initialized(&mut svm, &program_id, &sequencer.pubkey());

    // batch_num should be 1 (0 + 1), sending 2 instead
    let ix = usr_ix(program_id, &sequencer.pubkey(), state_key, &[1u8; 32], 2);
    let result = send(&mut svm, ix, &sequencer, &[&sequencer]);
    print_logs("usr_bad_batch_num", &result);
    assert_eq!(result.unwrap_err().err, custom_err(ERR_BAD_BATCH_NUM));
}

#[test]
fn test_usr_sequential_batches() {
    let (mut svm, program_id) = setup();
    let sequencer = Keypair::new();
    svm.airdrop(&sequencer.pubkey(), 1_000_000_000).unwrap();
    let state_key = setup_initialized(&mut svm, &program_id, &sequencer.pubkey());

    for batch in 1u64..=3 {
        svm.expire_blockhash();
        let root = [batch as u8; 32];
        let ix = usr_ix(program_id, &sequencer.pubkey(), state_key, &root, batch);
        send(&mut svm, ix, &sequencer, &[&sequencer]).unwrap();

        let d = svm.get_account(&state_key).unwrap().data;
        assert_eq!(&d[STATE_ROOT..STATE_ROOT + 32], &root);
        assert_eq!(
            &d[STATE_BATCH_NUM..STATE_BATCH_NUM + 8],
            &batch.to_le_bytes()
        );
    }
}
