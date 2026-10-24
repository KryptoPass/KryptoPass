# kp-vault

Vault application service over the signed `kp-sync` ledger. It depends on
`kp-core` for the strict encrypted object contract and injects its `OpenPgp`
port; there is no Qt, network transport, key generation, or new crypto runtime.

`SyncedVault` borrows an existing ledger, authorized local device identity,
trust source and OpenPGP implementation. `save` encrypts the full object and
atomically appends a revision with exact-head CAS; `delete` appends a persistent
tombstone. `list` returns every head, preserving concurrent branches.

The debug Desktop/Android exercise uses a separate public synthetic key and
ledger. It cannot export real private keys. See `docs/sin-revisar/sync/adaptadores-boveda-sync-v1.md`
and `docs/sin-revisar/sync/prueba-boveda-sincronizada-sintetica.md` in the application repositories.
