; L1 Rollup Program — sBPF Assembly
; Instructions: Initialize(0x00), UpdateStateRoot(0x01), Deposit(0x02), Withdraw(0x03)

; ── Input buffer ──────────────────────────────────────────────────────────────
.equ NUM_ACCOUNTS,              0x0

; ── Per-account field offsets (from account base) ─────────────────────────────
.equ ACCT_DUP,                  0x0    ; u8   0xFF = not dup
.equ ACCT_IS_SIGNER,            0x1    ; u8
.equ ACCT_IS_WRITE,             0x2    ; u8
.equ ACCT_EXEC,                 0x3    ; u8
.equ ACCT_KEY,                  0x8    ; [u8;32]
.equ ACCT_OWNER,                0x28   ; [u8;32]
.equ ACCT_LAMPORTS,             0x48   ; u64
.equ ACCT_DLEN,                 0x50   ; u64
.equ ACCT_DATA,                 0x58   ; data start

; ── State PDA data offsets (from ACCT_DATA) ───────────────────────────────────
; seeds = [b"state"]  total = 85 bytes
.equ STATE_INITIALIZED,         0x00   ; u8
.equ STATE_SEQ_PUBKEY,          0x01   ; [u8;32]
.equ STATE_ROOT,                0x21   ; [u8;32]
.equ STATE_BATCH_NUM,           0x41   ; u64
.equ STATE_VAULT_LAMPORTS,      0x49   ; u64
.equ STATE_BUMP,                0x51   ; u8   state PDA bump
.equ STATE_VAULT_BUMP,          0x52   ; u8   vault PDA bump
.equ STATE_WITHDRAW_MASK,       0x53   ; u16  withdrawal bitmask (10 bits, 1 per leaf slot)
.equ STATE_SZ,                  0x55   ; 85 bytes

; ── Instruction discriminators ────────────────────────────────────────────────
.equ IX_INITIALIZE,             0x0
.equ IX_UPDATE_STATE_ROOT,      0x1
.equ IX_DEPOSIT,                0x2
.equ IX_WITHDRAW,               0x3

; ── Initialize ix_data layout (35 bytes) ──────────────────────────────────────
; [0x00, seq_pubkey:32, state_bump:1, vault_bump:1]
.equ INIT_SEQ_PUBKEY,           0x1    ; [u8;32]
.equ INIT_STATE_BUMP,           0x21   ; u8
.equ INIT_VAULT_BUMP,           0x22   ; u8
.equ INIT_IX_LEN,               0x23   ; = 35

; ── UpdateStateRoot ix_data layout (41 bytes) ─────────────────────────────────
; [0x01, new_root:32, batch_num:8]
.equ USR_NEW_ROOT,              0x1    ; [u8;32]
.equ USR_BATCH_NUM,             0x21   ; u64
.equ USR_IX_LEN,                0x29   ; = 41

; ── Deposit ix_data layout (9 bytes) ──────────────────────────────────────────
; [0x02, amount:8]
.equ DEP_AMOUNT,                0x1    ; u64
.equ DEP_IX_LEN,                0x9    ; = 9

; ── Withdraw ix_data layout (158 bytes) ───────────────────────────────────────
; [0x03, amount:8, l2_lamports:8, l2_nonce:8, proof_index:4, vault_bump:1, siblings:128]
.equ WD_AMOUNT,                 0x01   ; u64  SOL to release
.equ WD_L2_LAMPORTS,            0x09   ; u64  L2 balance (for leaf hash)
.equ WD_L2_NONCE,               0x11   ; u64  L2 nonce   (for leaf hash)
.equ WD_PROOF_INDEX,            0x19   ; u32  leaf index = bitmask bit position
.equ WD_VAULT_BUMP,             0x1D   ; u8   vault PDA bump
.equ WD_SIBLINGS,               0x1E   ; [u8;32]*4 = 128 bytes
.equ WD_IX_LEN,                 0x9E   ; = 158

; ── Error codes ───────────────────────────────────────────────────────────────
.equ ERR_INVALID_IX,            0x1
.equ ERR_WRONG_ACCT_COUNT,      0x2
.equ ERR_NOT_SIGNER,            0x3
.equ ERR_NOT_WRITABLE,          0x4
.equ ERR_WRONG_ACCT_SIZE,       0x5
.equ ERR_NOT_INITIALIZED,       0x6
.equ ERR_ALREADY_INITIALIZED,   0x7
.equ ERR_WRONG_SEQUENCER,       0x8
.equ ERR_BAD_BATCH_NUM,         0x9
.equ ERR_INSUFFICIENT_VAULT,    0xA
.equ ERR_BAD_PROOF,             0xB
.equ ERR_ALREADY_WITHDRAWN,     0xC
.equ ERR_BAD_PDA,               0xD
.equ ERR_WITHDRAW_EXCEEDS_BAL,  0xE


.globl entrypoint

entrypoint:
    ; ── Dynamic account walk: save base ptrs to stack, land on ix_data ────────
    ldxdw r6, [r1 + NUM_ACCOUNTS]  ; r6 = num_accounts (loop counter)
    mov64 r7, r1
    add64 r7, 8                    ; r7 = cursor (starts at first account)
    mov64 r2, r10
    sub64 r2, 8                    ; r2 = descending stack ptr (saves account bases)

find_ix_data_loop:
    jeq   r6, 0, find_ix_data_done
    stxdw [r2 + 0], r7             ; save account base ptr
    sub64 r2, 8
    ldxdw r3, [r7 + ACCT_DLEN]
    add64 r3, 10247                ; data_len + 10240 + 7  (align8 prep, combined)
    mov64 r4, r3
    and64 r4, 7
    sub64 r3, r4                   ; align8(data_len + 10240)
    add64 r3, 96                   ; + fixed fields (88) + rent (8)
    add64 r7, r3                   ; advance cursor to next account
    sub64 r6, 1
    ja    find_ix_data_loop

find_ix_data_done:
    ; r7 = &{ ix_data_len: u64, ix_data: [u8] }
    ldxdw r3, [r7 + 0]             ; r3 = ix_data_len

    jlt   r3, 1, err_invalid_ix
    ldxb  r4, [r7 + 8]             ; r4 = discriminator

    jeq   r4, IX_INITIALIZE,        initialize
    jeq   r4, IX_UPDATE_STATE_ROOT, update_state_root
    jeq   r4, IX_DEPOSIT,           deposit
    jeq   r4, IX_WITHDRAW,          withdraw
    ja    err_invalid_ix

; ── Instruction stubs (to be implemented) ─────────────────────────────────────

initialize:
    ; data_len == 3
    ldxdw r3, [r1 + NUM_ACCOUNTS]
    jne r3, 3, err_wrong_acct_count

    ; ix_num_accts >= INIT_IX_LEN
    ldxdw r3, [r7 + 0]
    jlt r3, INIT_IX_LEN, err_invalid_ix

    ; acct0.is_signer == 1
    ldxdw r3, [r10 - 8]
    ldxb r2, [r3 + ACCT_IS_SIGNER]
    jne r2, 0x1, err_not_signer

    ldxdw r3, [r10 - 16]
    ; acct1.is_writable == 1
    ldxb r2, [r3 + ACCT_IS_WRITE]
    jne r2, 0x1, err_not_writable

    ; acct1.dlen == STATE_SZ
    ldxdw r2, [r3 + ACCT_DLEN]
    jne r2, STATE_SZ, err_wrong_acct_size

    ; acct1.data[STATE_INITIALIZED] == 0
    ldxdw r2, [r3 + ACCT_DATA + STATE_INITIALIZED]
    jne r2, 0x0, err_already_initialized
    
    

    lddw r0, 0
    exit

update_state_root:
    lddw r0, 0
    exit

deposit:
    lddw r0, 0
    exit

withdraw:
    lddw r0, 0
    exit

; ── Error handlers ────────────────────────────────────────────────────────────

err_invalid_ix:
    mov64 r0, ERR_INVALID_IX
    exit

err_wrong_acct_count:
    mov64 r0, ERR_WRONG_ACCT_COUNT
    exit

err_not_signer:
    mov64 r0, ERR_NOT_SIGNER
    exit

err_not_writable:
    mov64 r0, ERR_NOT_WRITABLE
    exit

err_wrong_acct_size:
    mov64 r0, ERR_WRONG_ACCT_SIZE
    exit

err_not_initialized:
    mov64 r0, ERR_NOT_INITIALIZED
    exit

err_already_initialized:
    mov64 r0, ERR_ALREADY_INITIALIZED
    exit

err_wrong_sequencer:
    mov64 r0, ERR_WRONG_SEQUENCER
    exit

err_bad_batch_num:
    mov64 r0, ERR_BAD_BATCH_NUM
    exit

err_insufficient_vault:
    mov64 r0, ERR_INSUFFICIENT_VAULT
    exit

err_bad_proof:
    mov64 r0, ERR_BAD_PROOF
    exit

err_already_withdrawn:
    mov64 r0, ERR_ALREADY_WITHDRAWN
    exit

err_bad_pda:
    mov64 r0, ERR_BAD_PDA
    exit

err_withdraw_exceeds_bal:
    mov64 r0, ERR_WITHDRAW_EXCEEDS_BAL
    exit

; ── Helpers ───────────────────────────────────────────────────────────────────

; cmp32: compare two 32-byte values
; r1 = ptr a,  r2 = ptr b
; returns r0 = 0 if equal, 1 if not
cmp32:
    ldxdw r3, [r1 + 0]
    ldxdw r4, [r2 + 0]
    jne   r3, r4, cmp32_ne
    ldxdw r3, [r1 + 8]
    ldxdw r4, [r2 + 8]
    jne   r3, r4, cmp32_ne
    ldxdw r3, [r1 + 16]
    ldxdw r4, [r2 + 16]
    jne   r3, r4, cmp32_ne
    ldxdw r3, [r1 + 24]
    ldxdw r4, [r2 + 24]
    jne   r3, r4, cmp32_ne
    mov64 r0, 0
    exit
cmp32_ne:
    mov64 r0, 1
    exit

; copy32: copy 32 bytes
; r1 = dst,  r2 = src
copy32:
    ldxdw r3, [r2 + 0]
    stxdw [r1 + 0],  r3
    ldxdw r3, [r2 + 8]
    stxdw [r1 + 8],  r3
    ldxdw r3, [r2 + 16]
    stxdw [r1 + 16], r3
    ldxdw r3, [r2 + 24]
    stxdw [r1 + 24], r3
    exit

; copy8: copy 8 bytes (u64)
; r1 = dst,  r2 = src
copy8:
    ldxdw r3, [r2 + 0]
    stxdw [r1 + 0],  r3
    exit

; zero32: zero 32 bytes
; r1 = dst
zero32:
    mov64 r2, 0
    stxdw [r1 + 0],  r2
    stxdw [r1 + 8],  r2
    stxdw [r1 + 16], r2
    stxdw [r1 + 24], r2
    exit

; sha256_pair: sha256(left || right) -> out
; r1 = left ptr  (32 bytes)
; r2 = right ptr (32 bytes)
; r3 = out ptr   (32 bytes)
; clobbers r1-r5
sha256_pair:
    mov64 r5, r10
    sub64 r5, 32               ; r5 = &vals[0]  — SolBytes[2] = 2*16 = 32 bytes
    stxdw [r5 + 0],  r1        ; vals[0].ptr = left
    lddw  r4, 32
    stxdw [r5 + 8],  r4        ; vals[0].len = 32
    stxdw [r5 + 16], r2        ; vals[1].ptr = right
    stxdw [r5 + 24], r4        ; vals[1].len = 32
    mov64 r1, r5               ; r1 = vals ptr
    lddw  r2, 2                ; r2 = nvals
    ; r3 = out ptr (unchanged)
    call sol_sha256
    exit

; sha256_leaf: sha256(pubkey || lamports || nonce) -> leaf hash
; r1 = pubkey ptr   (32 bytes)
; r2 = lamports ptr (8 bytes)
; r3 = nonce ptr    (8 bytes)
; r4 = out ptr      (32 bytes)
; clobbers r1-r5
sha256_leaf:
    mov64 r5, r10
    sub64 r5, 48               ; r5 = &vals[0]  — SolBytes[3] = 3*16 = 48 bytes
    stxdw [r5 + 0],  r1        ; vals[0].ptr = pubkey
    lddw  r0, 32
    stxdw [r5 + 8],  r0        ; vals[0].len = 32
    stxdw [r5 + 16], r2        ; vals[1].ptr = lamports
    lddw  r0, 8
    stxdw [r5 + 24], r0        ; vals[1].len = 8
    stxdw [r5 + 32], r3        ; vals[2].ptr = nonce
    stxdw [r5 + 40], r0        ; vals[2].len = 8
    mov64 r1, r5               ; r1 = vals ptr
    lddw  r2, 3                ; r2 = nvals
    mov64 r3, r4               ; r3 = out ptr
    call sol_sha256
    exit

; log_deposit: emit deposit event via sol_log_data
; r1 = pubkey ptr (32 bytes — acct0.key in input buffer)
; r2 = amount ptr (8 bytes  — points into ix_data)
; clobbers r1-r5
log_deposit:
    mov64 r5, r10
    sub64 r5, 32               ; r5 = &fields[0]  — SolBytes[2] = 32 bytes
    stxdw [r5 + 0],  r1        ; fields[0].ptr = pubkey
    lddw  r4, 32
    stxdw [r5 + 8],  r4        ; fields[0].len = 32
    stxdw [r5 + 16], r2        ; fields[1].ptr = amount
    lddw  r4, 8
    stxdw [r5 + 24], r4        ; fields[1].len = 8
    mov64 r1, r5               ; r1 = fields ptr
    lddw  r2, 2                ; r2 = nfields
    call sol_log_data
    exit

; fill_meta: r1=dst, r2=acct_ptr, r3=is_writable, r4=is_signer
fill_meta:
    add64 r2, ACCT_KEY
    stxdw [r1 + 0], r2
    stxb  [r1 + 8], r3
    stxb  [r1 + 9], r4
    exit

; fill_acct_info: r1=dst, r2=acct_ptr, r3=next_acct_ptr, r4=is_signer, r5=is_writable
; uses r0 as scratch, does NOT touch r6-r9
fill_acct_info:
    mov64 r0, r2
    add64 r0, ACCT_KEY
    stxdw [r1 + 0], r0
    mov64 r0, r2
    add64 r0, ACCT_LAMPORTS
    stxdw [r1 + 8], r0
    ldxdw r0, [r2 + ACCT_DLEN]
    stxdw [r1 + 16], r0
    mov64 r0, r2
    add64 r0, ACCT_DATA
    stxdw [r1 + 24], r0
    mov64 r0, r2
    add64 r0, ACCT_OWNER
    stxdw [r1 + 32], r0
    ldxdw r0, [r3 - 8]
    stxdw [r1 + 40], r0
    stxb  [r1 + 48], r4
    stxb  [r1 + 49], r5
    mov64 r0, 0
    stxb  [r1 + 50], r0
    exit
