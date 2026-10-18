use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use kryptopass_identity::{DeviceIdentity, SqliteTrustStore};
use kryptopass_pairing::{
    LocalApproval, LocalDecision, PairingConfig, PairingEngine, PairingInvitation, PairingMode,
    PairingRole, SasDisplay,
};
use kryptopass_transport::{MemoryTransport, MemoryTransportConfig};

struct SimulatedHumanApproval {
    node_name: &'static str,
}

#[async_trait]
impl LocalApproval for SimulatedHumanApproval {
    async fn confirm_sas(&mut self, sas: &SasDisplay) -> LocalDecision {
        let emojis = sas
            .emojis
            .iter()
            .map(|entry| entry.emoji)
            .collect::<Vec<_>>()
            .join(" ");
        let names = sas.names().join(", ");
        println!(
            "{} simulated a human SAS comparison: {} ({})",
            self.node_name, emojis, names
        );
        LocalDecision::Approve
    }
}

struct TempProfile(PathBuf);

impl TempProfile {
    fn create(label: &str) -> Result<Self, Box<dyn Error>> {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random)?;
        let suffix = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = std::env::temp_dir().join(format!("kryptopass-pairing-{label}-{suffix}"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700).create(&path)?;
        }
        #[cfg(not(unix))]
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempProfile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn now_ms() -> Result<i64, Box<dyn Error>> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok(i64::try_from(now.as_millis())?)
}

fn fresh_session_id() -> Result<[u8; 16], Box<dyn Error>> {
    let mut session_id = [0_u8; 16];
    getrandom::fill(&mut session_id)?;
    if session_id.iter().all(|byte| *byte == 0) {
        session_id[0] = 1;
    }
    Ok(session_id)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let alice_profile = TempProfile::create("alice")?;
    let bob_profile = TempProfile::create("bob")?;
    let alice_identity = DeviceIdentity::generate()?;
    let bob_identity = DeviceIdentity::generate()?;
    let now = now_ms()?;

    let alice_store = SqliteTrustStore::open_new(&alice_profile.0, alice_identity.device_id())?;
    let mut bob_store = SqliteTrustStore::open_new(&bob_profile.0, bob_identity.device_id())?;
    let hosted_invitation =
        PairingInvitation::create(&mut bob_store, now, Duration::from_secs(5 * 60))?;
    let qr = hosted_invitation.to_qr_bytes()?;
    let scanned_invitation = PairingInvitation::from_qr(qr.as_bytes())?;
    drop(qr);

    let session_id = fresh_session_id()?;
    let config = MemoryTransportConfig::default();
    let (alice_transport, bob_transport) = MemoryTransport::pair(config)?;
    let mut alice_approval = SimulatedHumanApproval { node_name: "Alice" };
    let mut bob_approval = SimulatedHumanApproval { node_name: "Bob" };
    let alice_engine = PairingEngine::new(
        alice_transport,
        &alice_identity,
        alice_store,
        PairingConfig::pairing(session_id, PairingRole::Initiator, now_ms()?),
    )?;
    let bob_engine = PairingEngine::new(
        bob_transport,
        &bob_identity,
        bob_store,
        PairingConfig::pairing(session_id, PairingRole::Responder, now_ms()?),
    )?;
    let (alice_pairing, bob_pairing) = tokio::join!(
        alice_engine.run(Some(&scanned_invitation), &mut alice_approval),
        bob_engine.run(Some(&hosted_invitation), &mut bob_approval),
    );
    let mut alice_pairing = alice_pairing?;
    let mut bob_pairing = bob_pairing?;
    println!("Pairing committed locally on Alice and Bob.");

    let alice_store =
        SqliteTrustStore::open_existing(&alice_profile.0, alice_identity.device_id())?;
    let bob_store = SqliteTrustStore::open_existing(&bob_profile.0, bob_identity.device_id())?;
    let alice_trusts_bob = alice_store.lookup_trusted_peer(&bob_identity.device_id())?;
    let bob_trusts_alice = bob_store.lookup_trusted_peer(&alice_identity.device_id())?;
    println!(
        "Reopened trust records: Alice trusts {}, Bob trusts {}.",
        alice_trusts_bob.device_id, bob_trusts_alice.device_id
    );

    alice_pairing.session.close().await?;
    bob_pairing.session.close().await?;

    let reconnect_session_id = fresh_session_id()?;
    let (alice_transport, bob_transport) = MemoryTransport::pair(MemoryTransportConfig::default())?;
    let alice_store =
        SqliteTrustStore::open_existing(&alice_profile.0, alice_identity.device_id())?;
    let bob_store = SqliteTrustStore::open_existing(&bob_profile.0, bob_identity.device_id())?;
    let alice_engine = PairingEngine::new(
        alice_transport,
        &alice_identity,
        alice_store,
        PairingConfig::reconnect(
            reconnect_session_id,
            PairingRole::Initiator,
            now_ms()?,
            alice_trusts_bob,
        ),
    )?;
    let bob_engine = PairingEngine::new(
        bob_transport,
        &bob_identity,
        bob_store,
        PairingConfig::reconnect(
            reconnect_session_id,
            PairingRole::Responder,
            now_ms()?,
            bob_trusts_alice,
        ),
    )?;
    let (alice_reconnect, bob_reconnect) = tokio::join!(
        alice_engine.run(None, &mut alice_approval),
        bob_engine.run(None, &mut bob_approval),
    );
    let mut alice_reconnect = alice_reconnect?;
    let mut bob_reconnect = bob_reconnect?;
    assert_eq!(alice_reconnect.mode, PairingMode::Reconnect);
    assert_eq!(bob_reconnect.mode, PairingMode::Reconnect);
    assert!(alice_reconnect.sas.is_none() && bob_reconnect.sas.is_none());
    println!("Fresh Noise reconnect authenticated both stored peers without SAS.");
    alice_reconnect.session.close().await?;
    bob_reconnect.session.close().await?;
    Ok(())
}
