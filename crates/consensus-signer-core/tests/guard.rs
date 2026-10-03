use std::{cell::Cell, collections::BTreeMap};

use consensus_signer_core::{
    ConsensusSignature, ConsensusSignerError, ConsensusSigningGuard, Reservation, SigningJournal,
    VoteIntent, VotePayloadEncoder, VoteSigner, VoteSlot, VoteStage,
};
use ledger_core::NetworkId;
use network_membership_core::ConsensusPublicKey;
use state_sync_core::BlockHash;

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const KEY: ConsensusPublicKey = ConsensusPublicKey::new([2; 32]);

#[derive(Default)]
struct MemoryJournal {
    votes: BTreeMap<VoteSlot, BlockHash>,
    fail: bool,
}

impl SigningJournal for MemoryJournal {
    type Error = ();

    fn reserve(&mut self, intent: VoteIntent) -> Result<Reservation, Self::Error> {
        if self.fail {
            return Err(());
        }
        let target = intent.target_hash;
        match self.votes.get(&intent.slot) {
            None => {
                self.votes.insert(intent.slot, target);
                Ok(Reservation::New)
            }
            Some(existing) if *existing == target => Ok(Reservation::ExistingSame),
            Some(_) => Ok(Reservation::Conflict),
        }
    }
}

struct TestEncoder;

impl VotePayloadEncoder for TestEncoder {
    type Error = ();

    fn encode(&self, intent: VoteIntent) -> Result<Vec<u8>, Self::Error> {
        let mut output = Vec::new();
        output.extend_from_slice(intent.slot.network.as_bytes());
        output.extend_from_slice(&intent.slot.set_id.to_be_bytes());
        output.extend_from_slice(&intent.slot.height.to_be_bytes());
        output.extend_from_slice(&intent.slot.round.to_be_bytes());
        output.push(match intent.slot.stage {
            VoteStage::Proposal => 0,
            VoteStage::Prevote => 1,
            VoteStage::Precommit => 2,
        });
        output.extend_from_slice(intent.target_hash.as_bytes());
        Ok(output)
    }
}

struct TestSigner;

impl VoteSigner for TestSigner {
    type Error = ();

    fn sign(
        &self,
        public_key: ConsensusPublicKey,
        payload: &[u8],
    ) -> Result<ConsensusSignature, Self::Error> {
        let mut signature = [0_u8; 64];
        signature[..32].copy_from_slice(public_key.as_bytes());
        signature[32..].copy_from_slice(&payload[payload.len() - 32..]);
        Ok(ConsensusSignature::new(signature))
    }
}

struct FailOnceSigner(Cell<bool>);

impl VoteSigner for FailOnceSigner {
    type Error = ();

    fn sign(
        &self,
        _public_key: ConsensusPublicKey,
        _payload: &[u8],
    ) -> Result<ConsensusSignature, Self::Error> {
        if self.0.replace(false) {
            Err(())
        } else {
            Ok(ConsensusSignature::new([7; 64]))
        }
    }
}

fn intent(hash: u8) -> VoteIntent {
    VoteIntent {
        slot: VoteSlot {
            network: NETWORK,
            set_id: 5,
            height: 100,
            round: 7,
            stage: VoteStage::Precommit,
        },
        target_hash: BlockHash::new([hash; 32]),
    }
}

#[test]
fn identical_retry_is_allowed_but_conflicting_vote_fails_closed() {
    let mut guard = ConsensusSigningGuard::new(
        NETWORK,
        KEY,
        MemoryJournal::default(),
        TestEncoder,
        TestSigner,
    );
    assert!(!guard.sign(intent(3)).unwrap().repeated_request);
    assert!(guard.sign(intent(3)).unwrap().repeated_request);
    assert_eq!(
        guard.sign(intent(4)),
        Err(ConsensusSignerError::ConflictingVote)
    );
}

#[test]
fn journal_failure_and_wrong_network_never_reach_signing() {
    let journal = MemoryJournal {
        fail: true,
        ..MemoryJournal::default()
    };
    let mut guard = ConsensusSigningGuard::new(NETWORK, KEY, journal, TestEncoder, TestSigner);
    assert_eq!(
        guard.sign(intent(3)),
        Err(ConsensusSignerError::JournalFailure)
    );

    let mut wrong = intent(3);
    wrong.slot.network = NetworkId::new([9; 32]);
    assert_eq!(guard.sign(wrong), Err(ConsensusSignerError::WrongNetwork));
}

#[test]
fn prevote_and_precommit_have_independent_double_sign_slots() {
    let mut guard = ConsensusSigningGuard::new(
        NETWORK,
        KEY,
        MemoryJournal::default(),
        TestEncoder,
        TestSigner,
    );
    let precommit = intent(3);
    let mut prevote = precommit;
    prevote.slot.stage = VoteStage::Prevote;

    assert!(guard.sign(prevote).is_ok());
    assert!(guard.sign(precommit).is_ok());
}

#[test]
fn the_same_round_and_stage_are_independent_at_the_next_height() {
    let mut guard = ConsensusSigningGuard::new(
        NETWORK,
        KEY,
        MemoryJournal::default(),
        TestEncoder,
        TestSigner,
    );
    let first = intent(3);
    let mut next_height = intent(4);
    next_height.slot.height = first.slot.height + 1;

    assert!(guard.sign(first).is_ok());
    assert!(guard.sign(next_height).is_ok());
}

#[test]
fn signer_failure_keeps_reservation_and_only_same_target_can_retry() {
    let mut guard = ConsensusSigningGuard::new(
        NETWORK,
        KEY,
        MemoryJournal::default(),
        TestEncoder,
        FailOnceSigner(Cell::new(true)),
    );
    assert_eq!(
        guard.sign(intent(3)),
        Err(ConsensusSignerError::SignerFailure)
    );
    assert!(guard.sign(intent(3)).unwrap().repeated_request);
    assert_eq!(
        guard.sign(intent(4)),
        Err(ConsensusSignerError::ConflictingVote)
    );
}
