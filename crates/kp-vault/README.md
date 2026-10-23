# kp-vault

Vault application service over the signed `kp-sync` ledger. It depends on
`kp-core` for the strict encrypted object contract and injects its `OpenPgp`
port; there is no Qt, network transport, key generation, or new crypto runtime.

`SyncedVault` borrows an existing ledger, authorized local device identity,
trust source and OpenPGP implementation. `save` encrypts the full object and
atomically appends a revision with exact-head CAS; `delete` appends a persistent
tombstone. `list` returns every head, preserving concurrent branches.

`migrate_from` preflights an explicit immutable source snapshot and commits at
most 32 revisions/8 MiB into an empty destination in one transaction. It never
modifies the source or drops metadata/recipients it cannot represent. The
application must freeze source writers and persist its authority selection
before enabling this for real user vaults; that UI is not enabled yet.

The debug Desktop/Android exercise uses a separate public synthetic key and
ledger. It cannot export real private keys. See `docs/vault-sync-adapter-v1.md`
and `docs/synthetic-vault-sync-v1.md` in the application repositories.
