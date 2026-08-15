use super::*;

#[test]
fn test_zk_identity_generation() {
    let id = ZkIdentity::new();
    assert_ne!(id.commitment, [0u8; 32]);
    assert_ne!(id.secret, [0u8; 32]);
}

#[test]
fn test_nullifier_derivation() {
    let id = ZkIdentity::new();
    let n1 = id.derive_nullifier(1);
    let n2 = id.derive_nullifier(2);
    let n1_again = id.derive_nullifier(1);

    assert_ne!(n1, n2);
    assert_eq!(n1, n1_again);
}

#[test]
fn test_governance_voting_flow() {
    let mut engine = GovernanceEngine::new();
    let proposal = Proposal {
        id: 1,
        title: "Test Proposal".to_string(),
        description: "Description".to_string(),
        author: "Author".to_string(),
        execution_hash: "0x123".to_string(),
        deadline: 1000,
    };

    engine.submit_proposal(proposal.clone());
    let proposals = engine.list_proposals();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].id, 1);

    let id = ZkIdentity::new();
    let nullifier = id.derive_nullifier(1);
    let proof = id.generate_proof(1, true);
    
    let vote = Vote {
        proposal_id: 1,
        voter_hash: hex::encode(nullifier),
        commitment: hex::encode(id.commitment),
        approve: true,
        proof,
    };

    // First vote should succeed
    assert!(engine.cast_vote(vote.clone()));

    // Second vote with same nullifier should fail
    assert!(!engine.cast_vote(vote));

    // Vote for non-existent proposal should fail
    let id2 = ZkIdentity::new();
    let invalid_vote = Vote {
        proposal_id: 99,
        voter_hash: hex::encode(id2.derive_nullifier(99)),
        commitment: hex::encode(id2.commitment),
        approve: true,
        proof: id2.generate_proof(99, true),
    };
    assert!(!engine.cast_vote(invalid_vote));
}

#[test]
fn test_timelock_queueing() {
    let mut engine = GovernanceEngine::new();
    let proposal = Proposal {
        id: 1,
        title: "Timelock Test".to_string(),
        description: "Desc".to_string(),
        author: "Auth".to_string(),
        execution_hash: "0xabc".to_string(),
        deadline: 1000,
    };
    engine.submit_proposal(proposal);
    engine.queue_execution(1);
    
    let (_proposal, status) = engine.proposals.first().unwrap();
    assert!(matches!(status, ProposalStatus::Timelocked { .. }));
}

#[test]
fn test_proposal_execution() {
    let mut engine = GovernanceEngine::new();
    let proposal = Proposal {
        id: 1,
        title: "Execution Test".to_string(),
        description: "Desc".to_string(),
        author: "Auth".to_string(),
        execution_hash: "0xdeadbeef".to_string(),
        deadline: 1000,
    };
    engine.submit_proposal(proposal);
    
    // 1. Cannot execute if not timelocked
    assert!(engine.execute_proposal(1).is_err());

    // 2. Manually set timelock to the past
    let past_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() - 100;
    
    if let Some((_, status)) = engine.proposals.iter_mut().find(|(p, _)| p.id == 1) {
        *status = ProposalStatus::Timelocked { release_time: past_time };
    }

    // 3. Should execute successfully now
    let res = engine.execute_proposal(1);
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "0xdeadbeef");

    // 4. Status should be Executed
    let (_, status) = engine.proposals.first().unwrap();
    assert!(matches!(status, ProposalStatus::Executed));

    // 5. Cannot execute twice
    assert!(engine.execute_proposal(1).is_err());
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10))] // Reduce cases for expensive ZK tests
        #[test]
        fn test_nullifier_determinism(secret in prop::array::uniform32(0u8..), proposal_id in 0u64..1000) {
            let id = ZkIdentity {
                commitment: [0u8; 32], // Not used for nullifier
                nullifier_hash: [0u8; 32],
                secret,
                use_gpu: false,
            };
            
            let n1 = id.derive_nullifier(proposal_id);
            let n2 = id.derive_nullifier(proposal_id);
            prop_assert_eq!(n1, n2);
        }

        #[test]
        fn test_vote_verification_success(secret in prop::array::uniform32(0u8..), proposal_id in 0u64..100) {
            let mut id = ZkIdentity {
                commitment: [0u8; 32],
                nullifier_hash: [0u8; 32],
                secret,
                use_gpu: false,
            };
            
            // Re-derive commitment correctly for the test
            let config = poseidon_config();
            let secret_fr = Fr::from_be_bytes_mod_order(&secret);
            let commitment_fr = <CRH<Fr> as CRHScheme>::evaluate(&config, vec![secret_fr]).unwrap();
            id.commitment = commitment_fr.into_bigint().to_bytes_be().try_into().unwrap();

            let proof = id.generate_proof(proposal_id, true);
            let nullifier = id.derive_nullifier(proposal_id);
            
            let mut engine = GovernanceEngine::new();
            engine.submit_proposal(Proposal {
                id: proposal_id,
                title: "Prop".into(),
                description: "Desc".into(),
                author: "Auth".into(),
                execution_hash: "0x".into(),
                deadline: 10000,
            });

            let vote = Vote {
                proposal_id,
                voter_hash: hex::encode(nullifier),
                commitment: hex::encode(id.commitment),
                approve: true,
                proof,
            };

            prop_assert!(engine.cast_vote(vote));
        }

        #[test]
        fn test_vote_verification_failure_mismatched_commitment(secret in prop::array::uniform32(0u8..), wrong_secret in prop::array::uniform32(0u8..), proposal_id in 0u64..100) {
            prop_assume!(secret != wrong_secret);

            let mut id = ZkIdentity {
                commitment: [0u8; 32],
                nullifier_hash: [0u8; 32],
                secret,
                use_gpu: false,
            };
            
            let config = poseidon_config();
            let secret_fr = Fr::from_be_bytes_mod_order(&secret);
            let commitment_fr = <CRH<Fr> as CRHScheme>::evaluate(&config, vec![secret_fr]).unwrap();
            id.commitment = commitment_fr.into_bigint().to_bytes_be().try_into().unwrap();

            // Generate VALID proof with CORRECT secret
            let proof = id.generate_proof(proposal_id, true);
            let nullifier = id.derive_nullifier(proposal_id);
            
            let mut engine = GovernanceEngine::new();
            engine.submit_proposal(Proposal {
                id: proposal_id,
                title: "Prop".into(),
                description: "Desc".into(),
                author: "Auth".into(),
                execution_hash: "0x".into(),
                deadline: 10000,
            });

            // Use WRONG commitment in the vote object
            let wrong_secret_fr = Fr::from_be_bytes_mod_order(&wrong_secret);
            let wrong_commitment_fr = <CRH<Fr> as CRHScheme>::evaluate(&config, vec![wrong_secret_fr]).unwrap();
            let wrong_commitment: [u8; 32] = wrong_commitment_fr.into_bigint().to_bytes_be().try_into().unwrap();

            let vote = Vote {
                proposal_id,
                voter_hash: hex::encode(nullifier),
                commitment: hex::encode(wrong_commitment),
                approve: true,
                proof,
            };

            // Verification should fail because proof commitment != vote commitment
            prop_assert!(!engine.cast_vote(vote));
        }

        #[test]
        fn test_vote_verification_failure_wrong_nullifier(secret in prop::array::uniform32(0u8..), proposal_id in 0u64..100, wrong_proposal_id in 0u64..100) {
            prop_assume!(proposal_id != wrong_proposal_id);

            let mut id = ZkIdentity {
                commitment: [0u8; 32],
                nullifier_hash: [0u8; 32],
                secret,
                use_gpu: false,
            };
            
            let config = poseidon_config();
            let secret_fr = Fr::from_be_bytes_mod_order(&secret);
            let commitment_fr = <CRH<Fr> as CRHScheme>::evaluate(&config, vec![secret_fr]).unwrap();
            id.commitment = commitment_fr.into_bigint().to_bytes_be().try_into().unwrap();

            // Generate VALID proof for proposal_id
            let proof = id.generate_proof(proposal_id, true);
            
            let mut engine = GovernanceEngine::new();
            engine.submit_proposal(Proposal {
                id: proposal_id,
                title: "Prop".into(),
                description: "Desc".into(),
                author: "Auth".into(),
                execution_hash: "0x".into(),
                deadline: 10000,
            });

            // Try to use a nullifier from a DIFFERENT proposal
            let wrong_nullifier = id.derive_nullifier(wrong_proposal_id);

            let vote = Vote {
                proposal_id,
                voter_hash: hex::encode(wrong_nullifier),
                commitment: hex::encode(id.commitment),
                approve: true,
                proof,
            };

            // Verification should fail because proof nullifier != vote nullifier
            prop_assert!(!engine.cast_vote(vote));
        }

        #[test]
        fn test_vote_verification_failure_mismatched_signal(secret in prop::array::uniform32(0u8..), proposal_id in 0u64..100) {
            let mut id = ZkIdentity {
                commitment: [0u8; 32],
                nullifier_hash: [0u8; 32],
                secret,
                use_gpu: false,
            };
            
            let config = poseidon_config();
            let secret_fr = Fr::from_be_bytes_mod_order(&secret);
            let commitment_fr = <CRH<Fr> as CRHScheme>::evaluate(&config, vec![secret_fr]).unwrap();
            id.commitment = commitment_fr.into_bigint().to_bytes_be().try_into().unwrap();

            // Generate VALID proof for approve=true
            let proof = id.generate_proof(proposal_id, true);
            let nullifier = id.derive_nullifier(proposal_id);
            
            let mut engine = GovernanceEngine::new();
            engine.submit_proposal(Proposal {
                id: proposal_id,
                title: "Prop".into(),
                description: "Desc".into(),
                author: "Auth".into(),
                execution_hash: "0x".into(),
                deadline: 10000,
            });

            // Try to cast vote with approve=false using the proof for approve=true
            let vote = Vote {
                proposal_id,
                voter_hash: hex::encode(nullifier),
                commitment: hex::encode(id.commitment),
                approve: false,
                proof,
            };

            // Verification should fail because proof signal (true) != vote signal (false)
            prop_assert!(!engine.cast_vote(vote));
        }

        #[test]
        fn test_vote_verification_failure_tampered_proof(secret in prop::array::uniform32(0u8..), proposal_id in 0u64..100) {
            let mut id = ZkIdentity {
                commitment: [0u8; 32],
                nullifier_hash: [0u8; 32],
                secret,
                use_gpu: false,
            };
            
            let config = poseidon_config();
            let secret_fr = Fr::from_be_bytes_mod_order(&secret);
            let commitment_fr = <CRH<Fr> as CRHScheme>::evaluate(&config, vec![secret_fr]).unwrap();
            id.commitment = commitment_fr.into_bigint().to_bytes_be().try_into().unwrap();

            let mut proof = id.generate_proof(proposal_id, true);
            if !proof.is_empty() {
                proof[0] ^= 0xFF; // Tamper with proof
            }
            
            let nullifier = id.derive_nullifier(proposal_id);
            
            let mut engine = GovernanceEngine::new();
            engine.submit_proposal(Proposal {
                id: proposal_id,
                title: "Prop".into(),
                description: "Desc".into(),
                author: "Auth".into(),
                execution_hash: "0x".into(),
                deadline: 10000,
            });

            let vote = Vote {
                proposal_id,
                voter_hash: hex::encode(nullifier),
                commitment: hex::encode(id.commitment),
                approve: true,
                proof,
            };

            prop_assert!(!engine.cast_vote(vote));
        }
    }
}
