use std::collections::{hash_map::Entry, HashMap};

use indy_api_types::errors::prelude::*;

use ursa::cl::{
    issuer::Issuer as UrsaIssuer, prover::Prover as UrsaProver, verifier::Verifier as UrsaVerifier,
    BlindedCredentialSecrets, BlindedCredentialSecretsCorrectnessProof, CredentialPublicKey,
    CredentialSecretsBlindingFactors, MasterSecret, SubProofRequest,
};

use crate::{
    domain::anoncreds::{
        credential::{AttributeValues, Credential},
        credential_attr_tag_policy::CredentialAttrTagPolicy,
        credential_definition::{
            CredentialDefinitionId, CredentialDefinitionV1 as CredentialDefinition,
        },
        credential_offer::CredentialOffer,
        credential_request::CredentialRequestMetadata,
        proof::{
            AttributeValue, Identifier, Proof, RequestedProof, RevealedAttributeGroupInfo,
            RevealedAttributeInfo, SubProofReferent,
        },
        proof_request::{
            PredicateInfo, PredicateTypes, ProofRequest, ProofRequestExtraQuery,
            ProofRequestPayload, ProofRequestsVersion, RequestedAttributeInfo,
            RequestedPredicateInfo,
        },
        requested_credential::{ProvingCredentialKey, RequestedCredentials},
        revocation_registry_definition::RevocationRegistryDefinitionV1,
        revocation_state::RevocationState,
        schema::{SchemaId, SchemaV1},
    },
    services::{AnoncredsHelpers, VerifierService},
    utils::wql::Query,
};

const ATTRIBUTE_EXISTENCE_MARKER: &str = "1";

pub struct ProverService {}

impl ProverService {
    pub fn new() -> ProverService {
        ProverService {}
    }

    pub fn new_master_secret(&self) -> IndyResult<MasterSecret> {
        trace!("new_master_secret > ");

        let master_secret = UrsaProver::new_master_secret()?;

        let res = Ok(master_secret);
        trace!("new_master_secret < {:?}", secret!(&res));
        res
    }

    pub fn new_credential_request(
        &self,
        cred_def: &CredentialDefinition,
        master_secret: &MasterSecret,
        credential_offer: &CredentialOffer,
    ) -> IndyResult<(
        BlindedCredentialSecrets,
        CredentialSecretsBlindingFactors,
        BlindedCredentialSecretsCorrectnessProof,
    )> {
        trace!(
            "new_credential_request > cred_def {:?} master_secret {:?} credential_offer {:?}",
            cred_def,
            secret!(&master_secret),
            credential_offer
        );

        let credential_pub_key = CredentialPublicKey::build_from_parts(
            &cred_def.value.primary,
            cred_def.value.revocation.as_ref(),
        )?;

        let cred_values = {
            let mut builder = UrsaIssuer::new_credential_values_builder()?;
            builder.add_value_hidden("master_secret", &master_secret.value()?)?;
            builder.finalize()?
        };

        let blinded_secrets = UrsaProver::blind_credential_secrets(
            &credential_pub_key,
            &credential_offer.key_correctness_proof,
            &cred_values,
            &credential_offer.nonce,
        )?;

        let res = Ok(blinded_secrets);
        trace!("new_credential_request < {:?}", res);
        res
    }

    pub fn process_credential(
        &self,
        credential: &mut Credential,
        cred_request_metadata: &CredentialRequestMetadata,
        master_secret: &MasterSecret,
        cred_def: &CredentialDefinition,
        rev_reg_def: Option<&RevocationRegistryDefinitionV1>,
    ) -> IndyResult<()> {
        trace!(
            "process_credential > credential {:?} cred_request_metadata {:?} \
                    master_secret {:?} cred_def {:?} rev_reg_def {:?}",
            credential,
            cred_request_metadata,
            secret!(&master_secret),
            cred_def,
            rev_reg_def
        );

        let credential_pub_key = CredentialPublicKey::build_from_parts(
            &cred_def.value.primary,
            cred_def.value.revocation.as_ref(),
        )?;

        let credential_values =
            AnoncredsHelpers::build_credential_values(&credential.values.0, Some(master_secret))?;

        UrsaProver::process_credential_signature(
            &mut credential.signature,
            &credential_values,
            &credential.signature_correctness_proof,
            &cred_request_metadata.master_secret_blinding_data,
            &credential_pub_key,
            &cred_request_metadata.nonce,
            rev_reg_def
                .as_ref()
                .map(|r_reg_def| &r_reg_def.value.public_keys.accum_key),
            credential.rev_reg.as_ref(),
            credential.witness.as_ref(),
        )?;

        let res = Ok(());
        trace!("process_credential < {:?}", res);
        res
    }

    pub fn create_proof(
        &self,
        credentials: &HashMap<String, Credential>,
        proof_req: &ProofRequest,
        requested_credentials: &RequestedCredentials,
        master_secret: &MasterSecret,
        schemas: &HashMap<SchemaId, SchemaV1>,
        cred_defs: &HashMap<CredentialDefinitionId, CredentialDefinition>,
        rev_states: &HashMap<String, HashMap<u64, RevocationState>>,
    ) -> IndyResult<Proof> {
        trace!(
            "create_proof > credentials {:?} proof_req {:?} \
                    requested_credentials {:?} master_secret {:?} \
                    schemas {:?} cred_defs {:?} rev_states {:?}",
            credentials,
            proof_req,
            requested_credentials,
            secret!(&master_secret),
            schemas,
            cred_defs,
            rev_states
        );

        let proof_req_val = proof_req.value();

        let mut proof_builder = {
            let mut builder = UrsaProver::new_proof_builder()?;
            builder.add_common_attribute("master_secret")?;
            builder
        };

        let mut requested_proof = {
            let mut rp = RequestedProof::default();
            rp.self_attested_attrs = requested_credentials.self_attested_attributes.clone();
            rp.unrevealed_attrs = requested_credentials.unrevealed_attrs.clone();
            rp
        };

        let credentials_for_proving =
            Self::_prepare_credentials_for_proving(requested_credentials, proof_req_val)?;

        let non_credential_schema = AnoncredsHelpers::build_non_credential_schema()?;
        let mut identifiers: Vec<Identifier> = Vec::with_capacity(credentials_for_proving.len());
        let mut sub_proof_index = 0;

        for (cred_key, (req_attrs_for_cred, req_predicates_for_cred)) in credentials_for_proving {
            let credential = credentials.get(cred_key.cred_id.as_str()).ok_or_else(|| {
                err_msg(
                    IndyErrorKind::InvalidStructure,
                    format!("Credential not found by id {:?}", cred_key.cred_id),
                )
            })?;

            let schema = schemas.get(&credential.schema_id).ok_or_else(|| {
                err_msg(
                    IndyErrorKind::InvalidStructure,
                    format!("Schema not found by id {:?}", credential.schema_id),
                )
            })?;

            let cred_def = cred_defs.get(&credential.cred_def_id).ok_or_else(|| {
                err_msg(
                    IndyErrorKind::InvalidStructure,
                    format!(
                        "CredentialDefinition not found by id {:?}",
                        credential.cred_def_id
                    ),
                )
            })?;

            let rev_state = if let Some(timestamp) = cred_key.timestamp {
                let rev_reg_id = credential.rev_reg_id.clone().ok_or_else(|| {
                    err_msg(
                        IndyErrorKind::InvalidStructure,
                        "Revocation Registry Id not found",
                    )
                })?;

                let rev_states_for_timestamp = rev_states
                    .get(&rev_reg_id.0)
                    .or(rev_states.get(cred_key.cred_id.as_str()))
                    .ok_or_else(|| {
                        err_msg(
                            IndyErrorKind::InvalidStructure,
                            format!("RevocationState not found by id {:?}", rev_reg_id),
                        )
                    })?;

                Some(rev_states_for_timestamp.get(&timestamp).ok_or_else(|| {
                    err_msg(
                        IndyErrorKind::InvalidStructure,
                        format!("RevocationInfo not found by timestamp {:?}", timestamp),
                    )
                })?)
            } else {
                None
            };

            let credential_pub_key = CredentialPublicKey::build_from_parts(
                &cred_def.value.primary,
                cred_def.value.revocation.as_ref(),
            )?;

            let credential_schema =
                AnoncredsHelpers::build_credential_schema(&schema.attr_names.0)?;

            let credential_values = AnoncredsHelpers::build_credential_values(
                &credential.values.0,
                Some(master_secret),
            )?;

            let sub_proof_request =
                Self::_build_sub_proof_request(&req_attrs_for_cred, &req_predicates_for_cred)?;
            proof_builder.add_sub_proof_request(
                &sub_proof_request,
                &credential_schema,
                &non_credential_schema,
                &credential.signature,
                &credential_values,
                &credential_pub_key,
                rev_state.as_ref().map(|r_info| &r_info.rev_reg),
                rev_state.as_ref().map(|r_info| &r_info.witness),
            )?;

            let identifier = match proof_req {
                ProofRequest::ProofRequestV1(_) => Identifier {
                    schema_id: credential.schema_id.to_unqualified(),
                    cred_def_id: credential.cred_def_id.to_unqualified(),
                    rev_reg_id: credential.rev_reg_id.as_ref().map(|id| id.to_unqualified()),
                    timestamp: cred_key.timestamp,
                },
                ProofRequest::ProofRequestV2(_) => Identifier {
                    schema_id: credential.schema_id.clone(),
                    cred_def_id: credential.cred_def_id.clone(),
                    rev_reg_id: credential.rev_reg_id.clone(),
                    timestamp: cred_key.timestamp,
                },
            };

            identifiers.push(identifier);

            Self::_update_requested_proof(
                req_attrs_for_cred,
                req_predicates_for_cred,
                proof_req_val,
                credential,
                sub_proof_index,
                &mut requested_proof,
            )?;

            sub_proof_index += 1;
        }

        let proof = proof_builder.finalize(&proof_req_val.nonce)?;

        let full_proof = Proof {
            proof,
            requested_proof,
            identifiers,
        };

        let res = Ok(full_proof);
        trace!("create_proof < {:?}", res);
        res
    }

    pub fn _prepare_credentials_for_proving(
        requested_credentials: &RequestedCredentials,
        proof_req: &ProofRequestPayload,
    ) -> IndyResult<
        HashMap<ProvingCredentialKey, (Vec<RequestedAttributeInfo>, Vec<RequestedPredicateInfo>)>,
    > {
        trace!(
            "_prepare_credentials_for_proving > requested_credentials {:?} proof_req {:?}",
            requested_credentials,
            proof_req
        );

        let mut credentials_for_proving: HashMap<
            ProvingCredentialKey,
            (Vec<RequestedAttributeInfo>, Vec<RequestedPredicateInfo>),
        > = HashMap::new();

        for (attr_referent, requested_attr) in requested_credentials.requested_attributes.iter() {
            let attr_info = proof_req
                .requested_attributes
                .get(attr_referent.as_str())
                .ok_or_else(|| {
                    err_msg(
                        IndyErrorKind::InvalidStructure,
                        format!(
                            "AttributeInfo not found in ProofRequest for referent \"{}\"",
                            attr_referent.as_str()
                        ),
                    )
                })?;

            let req_attr_info = RequestedAttributeInfo {
                attr_referent: attr_referent.clone(),
                attr_info: attr_info.clone(),
                revealed: requested_attr.revealed,
            };

            match credentials_for_proving.entry(ProvingCredentialKey {
                cred_id: requested_attr.cred_id.clone(),
                timestamp: requested_attr.timestamp,
            }) {
                Entry::Occupied(cred_for_proving) => {
                    let &mut (ref mut attributes_for_credential, _) = cred_for_proving.into_mut();
                    attributes_for_credential.push(req_attr_info);
                }
                Entry::Vacant(attributes_for_credential) => {
                    attributes_for_credential.insert((vec![req_attr_info], Vec::new()));
                }
            };
        }

        for (predicate_referent, proving_cred_key) in
            requested_credentials.requested_predicates.iter()
        {
            let predicate_info = proof_req
                .requested_predicates
                .get(predicate_referent.as_str())
                .ok_or_else(|| {
                    err_msg(
                        IndyErrorKind::InvalidStructure,
                        format!(
                            "PredicateInfo not found in ProofRequest for referent \"{}\"",
                            predicate_referent.as_str()
                        ),
                    )
                })?;

            let req_predicate_info = RequestedPredicateInfo {
                predicate_referent: predicate_referent.clone(),
                predicate_info: predicate_info.clone(),
            };

            match credentials_for_proving.entry(proving_cred_key.clone()) {
                Entry::Occupied(cred_for_proving) => {
                    let &mut (_, ref mut predicates_for_credential) = cred_for_proving.into_mut();
                    predicates_for_credential.push(req_predicate_info);
                }
                Entry::Vacant(v) => {
                    v.insert((Vec::new(), vec![req_predicate_info]));
                }
            };
        }

        let res = Ok(credentials_for_proving);
        trace!("_prepare_credentials_for_proving < {:?}", res);
        res
    }

    pub fn get_credential_values_for_attribute(
        &self,
        credential_attrs: &HashMap<String, AttributeValues>,
        requested_attr: &str,
    ) -> Option<AttributeValues> {
        trace!(
            "get_credential_values_for_attribute > credential_attrs {:?} requested_attr {:?}",
            credential_attrs,
            requested_attr
        );

        let res = Self::_get_credential_values_for_attribute(credential_attrs, requested_attr);

        trace!("get_credential_values_for_attribute < {:?}", res);
        res
    }

    fn _get_credential_values_for_attribute(
        credential_attrs: &HashMap<String, AttributeValues>,
        requested_attr: &str,
    ) -> Option<AttributeValues> {
        credential_attrs
            .iter()
            .find(|&(ref key, _)| {
                AnoncredsHelpers::attr_common_view(key)
                    == AnoncredsHelpers::attr_common_view(&requested_attr)
            })
            .map(|(_, values)| values.clone())
    }

    pub fn build_credential_tags(
        &self,
        credential: &Credential,
        catpol: Option<&CredentialAttrTagPolicy>,
    ) -> IndyResult<HashMap<String, String>> {
        trace!(
            "build_credential_tags > credential {:?} catpol {:?}",
            credential,
            catpol
        );

        let mut res: HashMap<String, String> = HashMap::new();

        let (schema_issuer_did, schema_name, schema_version) =
            credential.schema_id.parts().ok_or(IndyError::from_msg(
                IndyErrorKind::InvalidState,
                format!(
                    "Invalid Schema ID `{}`: wrong number of parts",
                    credential.schema_id.0
                ),
            ))?;

        let issuer_did = credential
            .cred_def_id
            .issuer_did()
            .ok_or(IndyError::from_msg(
                IndyErrorKind::InvalidState,
                format!(
                    "Invalid Credential Definition ID `{}`: wrong number of parts",
                    credential.cred_def_id.0
                ),
            ))?;

        res.insert("schema_id".to_string(), credential.schema_id.0.to_string());

        res.insert(
            "schema_issuer_did".to_string(),
            schema_issuer_did.0.to_string(),
        );

        res.insert("schema_name".to_string(), schema_name);
        res.insert("schema_version".to_string(), schema_version);
        res.insert("issuer_did".to_string(), issuer_did.0.to_string());

        res.insert(
            "cred_def_id".to_string(),
            credential.cred_def_id.0.to_string(),
        );

        res.insert(
            "rev_reg_id".to_string(),
            credential
                .rev_reg_id
                .as_ref()
                .map(|rev_reg_id| rev_reg_id.0.clone())
                .unwrap_or_else(|| "None".to_string()),
        );

        if credential.cred_def_id.is_fully_qualified() {
            res.insert(
                Credential::add_extra_tag_suffix("schema_id"),
                credential.schema_id.to_unqualified().0,
            );

            res.insert(
                Credential::add_extra_tag_suffix("schema_issuer_did"),
                schema_issuer_did.to_unqualified().0,
            );

            res.insert(
                Credential::add_extra_tag_suffix("issuer_did"),
                issuer_did.to_unqualified().0,
            );

            res.insert(
                Credential::add_extra_tag_suffix("cred_def_id"),
                credential.cred_def_id.to_unqualified().0,
            );

            res.insert(
                Credential::add_extra_tag_suffix("rev_reg_id"),
                credential
                    .rev_reg_id
                    .as_ref()
                    .map(|rev_reg_id| rev_reg_id.to_unqualified().0.clone())
                    .unwrap_or_else(|| "None".to_string()),
            );
        }

        credential.values.0.iter().for_each(|(attr, values)| {
            if catpol
                .map(|cp| cp.is_taggable(attr.as_str()))
                .unwrap_or(true)
            {
                // abstain for attrs policy marks untaggable
                res.insert(
                    Self::_build_attr_marker_tag(attr),
                    ATTRIBUTE_EXISTENCE_MARKER.to_string(),
                );

                res.insert(Self::_build_attr_value_tag(attr), values.raw.clone());
            }
        });

        let res = Ok(res);
        trace!("build_credential_tags < {:?}", res);
        res
    }

    fn _build_attr_marker_tag(attr: &str) -> String {
        format!(
            "attr::{}::marker",
            AnoncredsHelpers::attr_common_view(&attr)
        )
    }

    fn _build_attr_value_tag(attr: &str) -> String {
        format!("attr::{}::value", AnoncredsHelpers::attr_common_view(&attr))
    }

    pub fn attribute_satisfy_predicate(
        &self,
        predicate: &PredicateInfo,
        attribute_value: &str,
    ) -> IndyResult<bool> {
        trace!(
            "attribute_satisfy_predicate > predicate {:?} attribute_value {:?}",
            predicate,
            attribute_value
        );

        let res = match predicate.p_type {
            PredicateTypes::GE => {
                let attribute_value = attribute_value.parse::<i32>().to_indy(
                    IndyErrorKind::InvalidStructure,
                    format!(
                        "Credential attribute value \"{:?}\" is invalid",
                        attribute_value
                    ),
                )?;
                Ok(attribute_value >= predicate.p_value)
            }
            PredicateTypes::GT => {
                let attribute_value = attribute_value.parse::<i32>().to_indy(
                    IndyErrorKind::InvalidStructure,
                    format!(
                        "Credential attribute value \"{:?}\" is invalid",
                        attribute_value
                    ),
                )?;
                Ok(attribute_value > predicate.p_value)
            }
            PredicateTypes::LE => {
                let attribute_value = attribute_value.parse::<i32>().to_indy(
                    IndyErrorKind::InvalidStructure,
                    format!(
                        "Credential attribute value \"{:?}\" is invalid",
                        attribute_value
                    ),
                )?;
                Ok(attribute_value <= predicate.p_value)
            }
            PredicateTypes::LT => {
                let attribute_value = attribute_value.parse::<i32>().to_indy(
                    IndyErrorKind::InvalidStructure,
                    format!(
                        "Credential attribute value \"{:?}\" is invalid",
                        attribute_value
                    ),
                )?;
                Ok(attribute_value < predicate.p_value)
            }
        };

        trace!("attribute_satisfy_predicate < {:?}", res);
        res
    }

    fn _update_requested_proof(
        req_attrs_for_credential: Vec<RequestedAttributeInfo>,
        req_predicates_for_credential: Vec<RequestedPredicateInfo>,
        proof_req: &ProofRequestPayload,
        credential: &Credential,
        sub_proof_index: u32,
        requested_proof: &mut RequestedProof,
    ) -> IndyResult<()> {
        trace!(
            "_update_requested_proof > req_attrs_for_credential {:?} \
                    req_predicates_for_credential {:?} proof_req {:?} credential {:?} \
                sub_proof_index {:?} requested_proof {:?}",
            req_attrs_for_credential,
            req_predicates_for_credential,
            proof_req,
            credential,
            sub_proof_index,
            requested_proof
        );

        for attr_info in req_attrs_for_credential {
            if attr_info.revealed {
                let attribute = &proof_req.requested_attributes[&attr_info.attr_referent];

                if let Some(name) = &attribute.name {
                    let attribute_values =
                        Self::_get_credential_values_for_attribute(&credential.values.0, &name)
                            .ok_or_else(|| {
                                err_msg(
                                    IndyErrorKind::InvalidStructure,
                                    format!("Credential value not found for attribute {:?}", name),
                                )
                            })?;

                    requested_proof.revealed_attrs.insert(
                        attr_info.attr_referent.clone(),
                        RevealedAttributeInfo {
                            sub_proof_index,
                            raw: attribute_values.raw,
                            encoded: attribute_values.encoded,
                        },
                    );
                } else if let Some(names) = &attribute.names {
                    let mut value_map: HashMap<String, AttributeValue> = HashMap::new();

                    for name in names {
                        let attr_value =
                            Self::_get_credential_values_for_attribute(&credential.values.0, &name)
                                .ok_or_else(|| {
                                    err_msg(
                                        IndyErrorKind::InvalidStructure,
                                        format!(
                                            "Credential value not found for attribute {:?}",
                                            name
                                        ),
                                    )
                                })?;

                        value_map.insert(
                            name.clone(),
                            AttributeValue {
                                raw: attr_value.raw,
                                encoded: attr_value.encoded,
                            },
                        );
                    }

                    requested_proof.revealed_attr_groups.insert(
                        attr_info.attr_referent.clone(),
                        RevealedAttributeGroupInfo {
                            sub_proof_index,
                            values: value_map,
                        },
                    );
                }
            } else {
                requested_proof.unrevealed_attrs.insert(
                    attr_info.attr_referent,
                    SubProofReferent { sub_proof_index },
                );
            }
        }

        for predicate_info in req_predicates_for_credential {
            requested_proof.predicates.insert(
                predicate_info.predicate_referent,
                SubProofReferent { sub_proof_index },
            );
        }

        let res = Ok(());
        trace!("_update_requested_proof < {:?}", res);
        res
    }

    fn _build_sub_proof_request(
        req_attrs_for_credential: &[RequestedAttributeInfo],
        req_predicates_for_credential: &[RequestedPredicateInfo],
    ) -> IndyResult<SubProofRequest> {
        trace!(
            "_build_sub_proof_request > req_attrs_for_credential {:?} \
                    req_predicates_for_credential {:?}",
            req_attrs_for_credential,
            req_predicates_for_credential
        );

        let sub_proof_request = {
            let mut builder = UrsaVerifier::new_sub_proof_request_builder()?;

            for attr in req_attrs_for_credential {
                if !attr.revealed {
                    continue;
                }

                if let Some(ref name) = &attr.attr_info.name {
                    builder.add_revealed_attr(&AnoncredsHelpers::attr_common_view(name))?
                }

                if let Some(ref names) = &attr.attr_info.names {
                    for name in names {
                        builder.add_revealed_attr(&AnoncredsHelpers::attr_common_view(name))?
                    }
                }
            }

            for predicate in req_predicates_for_credential {
                builder.add_predicate(
                    &AnoncredsHelpers::attr_common_view(&predicate.predicate_info.name),
                    &predicate.predicate_info.p_type.to_string(),
                    predicate.predicate_info.p_value,
                )?;
            }

            builder.finalize()?
        };

        let res = Ok(sub_proof_request);
        trace!("_build_sub_proof_request < {:?}", res);
        res
    }

    pub fn process_proof_request_restrictions(
        &self,
        version: &ProofRequestsVersion,
        name: &Option<String>,
        names: &Option<Vec<String>>,
        referent: &str,
        restrictions: &Option<Query>,
        extra_query: &Option<&ProofRequestExtraQuery>,
    ) -> IndyResult<Query> {
        trace!(
            "process_proof_request_restrictions > version {:?} \
                    name {:?} names {:?} referent {:?} \
                    restrictions {:?} extra_query {:?}",
            version,
            name,
            names,
            referent,
            restrictions,
            extra_query
        );

        let mut queries: Vec<Query> = Vec::new();

        let mut attr_queries: Vec<Query> = name
            .iter()
            .chain(names.iter().flatten())
            .map(|name| {
                Query::Eq(
                    Self::_build_attr_marker_tag(name),
                    ATTRIBUTE_EXISTENCE_MARKER.to_string(),
                )
            })
            .collect();

        if attr_queries.is_empty() {
            Err(IndyError::from_msg(
                IndyErrorKind::InvalidStructure,
                r#"Proof Request attribute restriction should contain "name" or "names" param"#,
            ))?;
        }

        if let Some(restrictions_) = restrictions.clone() {
            match version {
                ProofRequestsVersion::V1 => {
                    let insensitive_restrictions =
                        Self::_make_restrictions_by_internal_tags_case_insensitive(restrictions_)?;
                    queries.push(self._double_restrictions(insensitive_restrictions)?)
                }
                ProofRequestsVersion::V2 => {
                    let insensitive_restrictions =
                        Self::_make_restrictions_by_internal_tags_case_insensitive(restrictions_)?;
                    queries.push(insensitive_restrictions)
                }
            };
        }

        if let Some(extra_query_) = extra_query.as_ref().and_then(|query| query.get(referent)) {
            queries.push(extra_query_.clone())
        }

        // put attr_queries last as this results in a better performing query with large datasets
        // ref IS-1470
        queries.append(&mut attr_queries);

        let res = Ok(Query::And(queries));
        trace!("process_proof_request_restrictions < {:?}", res);
        res
    }

    fn _make_restrictions_by_internal_tags_case_insensitive(operator: Query) -> IndyResult<Query> {
        let query = match operator {
            Query::Eq(tag_name, tag_value) => {
                if let Some(tag_name) = VerifierService::attr_request_by_value(&tag_name) {
                    Query::Eq(Self::_build_attr_value_tag(tag_name), tag_value)
                } else if let Some(tag_name) = VerifierService::attr_request_by_marker(&tag_name) {
                    Query::Eq(Self::_build_attr_marker_tag(tag_name), tag_value)
                } else {
                    Query::Eq(tag_name, tag_value)
                }
            }
            Query::Neq(tag_name, tag_value) => {
                if let Some(tag_name) = VerifierService::attr_request_by_value(&tag_name) {
                    Query::Neq(Self::_build_attr_value_tag(tag_name), tag_value)
                } else if let Some(tag_name) = VerifierService::attr_request_by_marker(&tag_name) {
                    Query::Neq(Self::_build_attr_marker_tag(tag_name), tag_value)
                } else {
                    Query::Neq(tag_name, tag_value)
                }
            }
            Query::In(tag_name, tag_values) => {
                if let Some(tag_name) = VerifierService::attr_request_by_value(&tag_name) {
                    Query::In(Self::_build_attr_value_tag(tag_name), tag_values)
                } else if let Some(tag_name) = VerifierService::attr_request_by_marker(&tag_name) {
                    Query::In(Self::_build_attr_marker_tag(tag_name), tag_values)
                } else {
                    Query::In(tag_name, tag_values)
                }
            }
            Query::And(operators) => Query::And(
                operators
                    .into_iter()
                    .map(|op| Self::_make_restrictions_by_internal_tags_case_insensitive(op))
                    .collect::<IndyResult<Vec<Query>>>()?,
            ),
            Query::Or(operators) => Query::Or(
                operators
                    .into_iter()
                    .map(|op| Self::_make_restrictions_by_internal_tags_case_insensitive(op))
                    .collect::<IndyResult<Vec<Query>>>()?,
            ),
            Query::Not(operator) => Query::Not(::std::boxed::Box::new(
                Self::_make_restrictions_by_internal_tags_case_insensitive(*operator)?,
            )),
            _ => {
                return Err(IndyError::from_msg(
                    IndyErrorKind::InvalidStructure,
                    "unsupported operator",
                ))
            }
        };

        Ok(query)
    }

    fn _double_restrictions(&self, operator: Query) -> IndyResult<Query> {
        let query = match operator {
            Query::Eq(tag_name, tag_value) => {
                if Credential::QUALIFIABLE_TAGS.contains(&tag_name.as_str()) {
                    Query::Or(vec![
                        Query::Eq(tag_name.clone(), tag_value.clone()),
                        Query::Eq(Credential::add_extra_tag_suffix(&tag_name), tag_value),
                    ])
                } else {
                    Query::Eq(tag_name, tag_value)
                }
            }
            Query::Neq(tag_name, tag_value) => {
                if Credential::QUALIFIABLE_TAGS.contains(&tag_name.as_str()) {
                    Query::And(vec![
                        Query::Neq(tag_name.clone(), tag_value.clone()),
                        Query::Neq(Credential::add_extra_tag_suffix(&tag_name), tag_value),
                    ])
                } else {
                    Query::Neq(tag_name, tag_value)
                }
            }
            Query::In(tag_name, tag_values) => {
                if Credential::QUALIFIABLE_TAGS.contains(&tag_name.as_str()) {
                    Query::Or(vec![
                        Query::In(tag_name.clone(), tag_values.clone()),
                        Query::In(Credential::add_extra_tag_suffix(&&tag_name), tag_values),
                    ])
                } else {
                    Query::In(tag_name, tag_values)
                }
            }
            Query::And(operators) => Query::And(
                operators
                    .into_iter()
                    .map(|op| self._double_restrictions(op))
                    .collect::<IndyResult<Vec<Query>>>()?,
            ),
            Query::Or(operators) => Query::Or(
                operators
                    .into_iter()
                    .map(|op| self._double_restrictions(op))
                    .collect::<IndyResult<Vec<Query>>>()?,
            ),
            Query::Not(operator) => Query::Not(::std::boxed::Box::new(
                self._double_restrictions(*operator)?,
            )),
            _ => {
                return Err(IndyError::from_msg(
                    IndyErrorKind::InvalidStructure,
                    "unsupported operator",
                ))
            }
        };

        Ok(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA_ID: &str = "NcYxiDXkpYi6ov5FcYDi1e:2:gvt:1.0";
    const SCHEMA_ISSUER_DID: &str = "NcYxiDXkpYi6ov5FcYDi1e";
    const SCHEMA_NAME: &str = "gvt";
    const SCHEMA_VERSION: &str = "1.0";
    const ISSUER_DID: &str = "NcYxiDXkpYi6ov5FcYDi1e";
    const CRED_DEF_ID: &str = "NcYxiDXkpYi6ov5FcYDi1e:3:CL:1:tag";
    const REV_REG_ID: &str = "NcYxiDXkpYi6ov5FcYDi1e:4:NcYxiDXkpYi6ov5FcYDi1e:3:CL:NcYxiDXkpYi6ov5FcYDi1e:2:gvt:1.0:tag:CL_ACCUM:TAG_1";
    const NO_REV_REG_ID: &str = "None";

    macro_rules! hashmap {
        ($( $key: expr => $val: expr ),*) => {
            {
                let mut map = ::std::collections::HashMap::new();
                $(
                    map.insert($key, $val);
                )*
                map
            }
        }
    }

    mod build_credential_tags {
        use super::*;
        use crate::domain::anoncreds::revocation_registry_definition::RevocationRegistryId;

        fn _credential() -> Credential {
            // note that encoding is not standardized by Indy except that 32-bit integers are encoded as themselves. IS-786
            // so Alex -> 12345 is an application choice while 25 -> 25 is not
            let mut attr_values: HashMap<String, AttributeValues> = HashMap::new();
            attr_values.insert(
                "name".to_string(),
                AttributeValues {
                    raw: "Alex".to_string(),
                    encoded: "12345".to_string(),
                },
            );
            attr_values.insert(
                "age".to_string(),
                AttributeValues {
                    raw: "25".to_string(),
                    encoded: "25".to_string(),
                },
            );

            serde_json::from_str::<Credential>(
                &json!({
                    "schema_id": SCHEMA_ID,
                    "cred_def_id": CRED_DEF_ID,
                    "values": attr_values,
                    "signature": json!({
                        "p_credential": json!({"m_2": "0","a": "0","e": "0","v": "0"})
                    }),
                    "signature_correctness_proof": json!({"se":"0", "c":"0"})
                })
                .to_string(),
            )
            .unwrap()
        }

        #[test]
        fn build_credential_tags_works() {
            let ps = ProverService::new();
            let tags = ps.build_credential_tags(&_credential(), None).unwrap();

            let expected_tags: HashMap<String, String> = hashmap!(
               "schema_id".to_string() => SCHEMA_ID.to_string(),
               "schema_issuer_did".to_string() => SCHEMA_ISSUER_DID.to_string(),
               "schema_name".to_string() => SCHEMA_NAME.to_string(),
               "schema_version".to_string() => SCHEMA_VERSION.to_string(),
               "issuer_did".to_string() => ISSUER_DID.to_string(),
               "cred_def_id".to_string() => CRED_DEF_ID.to_string(),
               "rev_reg_id".to_string() => NO_REV_REG_ID.to_string(),
               "attr::name::marker".to_string() => ATTRIBUTE_EXISTENCE_MARKER.to_string(),
               "attr::name::value".to_string() => "Alex".to_string(),
               "attr::age::marker".to_string() => ATTRIBUTE_EXISTENCE_MARKER.to_string(),
               "attr::age::value".to_string() => "25".to_string()
            );

            assert_eq!(expected_tags, tags)
        }

        #[test]
        fn build_credential_tags_works_for_catpol() {
            let ps = ProverService::new();
            let catpol = CredentialAttrTagPolicy::from(vec![String::from("name")]);
            let tags = ps
                .build_credential_tags(&_credential(), Some(catpol).as_ref())
                .unwrap();

            let expected_tags: HashMap<String, String> = hashmap!(
               "schema_id".to_string() => SCHEMA_ID.to_string(),
               "schema_issuer_did".to_string() => SCHEMA_ISSUER_DID.to_string(),
               "schema_name".to_string() => SCHEMA_NAME.to_string(),
               "schema_version".to_string() => SCHEMA_VERSION.to_string(),
               "issuer_did".to_string() => ISSUER_DID.to_string(),
               "cred_def_id".to_string() => CRED_DEF_ID.to_string(),
               "rev_reg_id".to_string() => NO_REV_REG_ID.to_string(),
               "attr::name::marker".to_string() => ATTRIBUTE_EXISTENCE_MARKER.to_string(),
               "attr::name::value".to_string() => "Alex".to_string()
            );

            assert_eq!(expected_tags, tags)
        }

        #[test]
        fn build_credential_tags_works_for_rev_reg_id() {
            let ps = ProverService::new();
            let mut credential = _credential();
            credential.rev_reg_id = Some(RevocationRegistryId(REV_REG_ID.to_string()));
            let tags = ps.build_credential_tags(&credential, None).unwrap();

            let expected_tags: HashMap<String, String> = hashmap!(
               "schema_id".to_string() => SCHEMA_ID.to_string(),
               "schema_issuer_did".to_string() => SCHEMA_ISSUER_DID.to_string(),
               "schema_name".to_string() => SCHEMA_NAME.to_string(),
               "schema_version".to_string() => SCHEMA_VERSION.to_string(),
               "issuer_did".to_string() => ISSUER_DID.to_string(),
               "cred_def_id".to_string() => CRED_DEF_ID.to_string(),
               "rev_reg_id".to_string() => REV_REG_ID.to_string(),
               "attr::name::marker".to_string() => ATTRIBUTE_EXISTENCE_MARKER.to_string(),
               "attr::name::value".to_string() => "Alex".to_string(),
               "attr::age::marker".to_string() => ATTRIBUTE_EXISTENCE_MARKER.to_string(),
               "attr::age::value".to_string() => "25".to_string()
            );

            assert_eq!(expected_tags, tags)
        }

        #[test]
        fn build_credential_tags_works_for_fully_qualified_ids() {
            let ps = ProverService::new();

            let schema_id = "did:indy:NcYxiDXkpYi6ov5FcYDi1e/anoncreds/v0/SCHEMA/gvt/1.0";
            let issuer_did = "did:indy:NcYxiDXkpYi6ov5FcYDi1e";
            let cred_def_id = "did:indy:NcYxiDXkpYi6ov5FcYDi1e/anoncreds/v0/CLAIM_DEF/1/tag";
            let rev_reg_id = "did:indy:NcYxiDXkpYi6ov5FcYDi1e/anoncreds/v0/REV_REG_DEF/did:indy:NcYxiDXkpYi6ov5FcYDi1e/anoncreds/v0/SCHEMA/gvt/1.0/tag/TAG_1";

            let mut credential = _credential();
            credential.schema_id = SchemaId(schema_id.to_string());
            credential.cred_def_id = CredentialDefinitionId(cred_def_id.to_string());
            credential.rev_reg_id = Some(RevocationRegistryId(rev_reg_id.to_string()));

            let tags = ps.build_credential_tags(&credential, None).unwrap();

            let expected_tags: HashMap<String, String> = hashmap!(
               "schema_id".to_string() => schema_id.to_string(),
               "schema_id_short".to_string() => SCHEMA_ID.to_string(),
               "schema_issuer_did".to_string() => issuer_did.to_string(),
               "schema_issuer_did_short".to_string() => ISSUER_DID.to_string(),
               "schema_name".to_string() => SCHEMA_NAME.to_string(),
               "schema_version".to_string() => SCHEMA_VERSION.to_string(),
               "issuer_did".to_string() => issuer_did.to_string(),
               "issuer_did_short".to_string() => ISSUER_DID.to_string(),
               "cred_def_id".to_string() => cred_def_id.to_string(),
               "cred_def_id_short".to_string() => CRED_DEF_ID.to_string(),
               "rev_reg_id".to_string() => rev_reg_id.to_string(),
               "rev_reg_id_short".to_string() => REV_REG_ID.to_string(),
               "attr::name::marker".to_string() => ATTRIBUTE_EXISTENCE_MARKER.to_string(),
               "attr::name::value".to_string() => "Alex".to_string(),
               "attr::age::marker".to_string() => ATTRIBUTE_EXISTENCE_MARKER.to_string(),
               "attr::age::value".to_string() => "25".to_string()
            );

            assert_eq!(expected_tags, tags)
        }
    }

    mod attribute_satisfy_predicate {
        use super::*;

        fn predicate_info() -> PredicateInfo {
            PredicateInfo {
                name: "age".to_string(),
                p_type: PredicateTypes::GE,
                p_value: 8,
                restrictions: None,
                non_revoked: None,
            }
        }

        #[test]
        fn attribute_satisfy_predicate_works() {
            let ps = ProverService::new();
            let res = ps
                .attribute_satisfy_predicate(&predicate_info(), "10")
                .unwrap();
            assert!(res);
        }

        #[test]
        fn attribute_satisfy_predicate_works_for_false() {
            let ps = ProverService::new();
            let res = ps
                .attribute_satisfy_predicate(&predicate_info(), "5")
                .unwrap();
            assert!(!res);
        }

        #[test]
        fn attribute_satisfy_predicate_works_for_invalid_attribute_value() {
            let ps = ProverService::new();
            let res = ps.attribute_satisfy_predicate(&predicate_info(), "string");
            assert_kind!(IndyErrorKind::InvalidStructure, res);
        }
    }

    mod prepare_credentials_for_proving {
        use crate::domain::anoncreds::{
            proof_request::{AttributeInfo, PredicateInfo},
            requested_credential::RequestedAttribute,
        };

        use super::*;

        const CRED_ID: &str = "8591bcac-ee7d-4bef-ba7e-984696440b30";
        const ATTRIBUTE_REFERENT: &str = "attribute_referent";
        const PREDICATE_REFERENT: &str = "predicate_referent";

        fn _attr_info() -> AttributeInfo {
            AttributeInfo {
                name: Some("name".to_string()),
                names: None,
                restrictions: None,
                non_revoked: None,
            }
        }

        fn _predicate_info() -> PredicateInfo {
            PredicateInfo {
                name: "age".to_string(),
                p_type: PredicateTypes::GE,
                p_value: 8,
                restrictions: None,
                non_revoked: None,
            }
        }

        fn _proof_req() -> ProofRequestPayload {
            ProofRequestPayload {
                nonce: ursa::cl::new_nonce().unwrap(),
                name: "Job-Application".to_string(),
                version: "0.1".to_string(),
                requested_attributes: hashmap!(
                    ATTRIBUTE_REFERENT.to_string() => _attr_info()
                ),
                requested_predicates: hashmap!(
                    PREDICATE_REFERENT.to_string() => _predicate_info()
                ),
                non_revoked: None,
            }
        }

        fn _req_cred() -> RequestedCredentials {
            RequestedCredentials {
                self_attested_attributes: HashMap::new(),
                requested_attributes: hashmap!(
                    ATTRIBUTE_REFERENT.to_string() => RequestedAttribute{
                        cred_id: CRED_ID.to_string(),
                        timestamp: None,
                        revealed: false,
                    }
                ),
                requested_predicates: hashmap!(
                    PREDICATE_REFERENT.to_string() => ProvingCredentialKey{ cred_id: CRED_ID.to_string(), timestamp: None }
                ),
            }
        }

        #[test]
        fn prepare_credentials_for_proving_works() {
            let req_cred = _req_cred();
            let proof_req = _proof_req();

            let res =
                ProverService::_prepare_credentials_for_proving(&req_cred, &proof_req).unwrap();

            assert_eq!(1, res.len());
            assert!(res.contains_key(&ProvingCredentialKey {
                cred_id: CRED_ID.to_string(),
                timestamp: None
            }));

            let (req_attr_info, req_pred_info) = res
                .get(&ProvingCredentialKey {
                    cred_id: CRED_ID.to_string(),
                    timestamp: None,
                })
                .unwrap();
            assert_eq!(1, req_attr_info.len());
            assert_eq!(1, req_pred_info.len());
        }

        #[test]
        fn prepare_credentials_for_proving_works_for_multiple_attributes_with_same_credential() {
            let mut req_cred = _req_cred();
            let mut proof_req = _proof_req();

            req_cred.requested_attributes.insert(
                "attribute_referent_2".to_string(),
                RequestedAttribute {
                    cred_id: CRED_ID.to_string(),
                    timestamp: None,
                    revealed: false,
                },
            );

            proof_req.requested_attributes.insert(
                "attribute_referent_2".to_string(),
                AttributeInfo {
                    name: Some("last_name".to_string()),
                    names: None,
                    restrictions: None,
                    non_revoked: None,
                },
            );

            let res =
                ProverService::_prepare_credentials_for_proving(&req_cred, &proof_req).unwrap();

            assert_eq!(1, res.len());
            assert!(res.contains_key(&ProvingCredentialKey {
                cred_id: CRED_ID.to_string(),
                timestamp: None
            }));

            let (req_attr_info, req_pred_info) = res
                .get(&ProvingCredentialKey {
                    cred_id: CRED_ID.to_string(),
                    timestamp: None,
                })
                .unwrap();
            assert_eq!(2, req_attr_info.len());
            assert_eq!(1, req_pred_info.len());
        }

        #[test]
        fn prepare_credentials_for_proving_works_for_missed_attribute() {
            let req_cred = _req_cred();
            let mut proof_req = _proof_req();

            proof_req.requested_attributes.clear();

            let res = ProverService::_prepare_credentials_for_proving(&req_cred, &proof_req);
            assert_kind!(IndyErrorKind::InvalidStructure, res);
        }

        #[test]
        fn prepare_credentials_for_proving_works_for_missed_predicate() {
            let req_cred = _req_cred();
            let mut proof_req = _proof_req();

            proof_req.requested_predicates.clear();

            let res = ProverService::_prepare_credentials_for_proving(&req_cred, &proof_req);
            assert_kind!(IndyErrorKind::InvalidStructure, res);
        }
    }

    mod get_credential_values_for_attribute {
        use super::*;

        fn _attr_values() -> AttributeValues {
            AttributeValues {
                raw: "Alex".to_string(),
                encoded: "123".to_string(),
            }
        }

        fn _cred_values() -> HashMap<String, AttributeValues> {
            hashmap!("name".to_string() => _attr_values())
        }

        #[test]
        fn get_credential_values_for_attribute_works() {
            let ps = ProverService::new();

            let res = ps
                .get_credential_values_for_attribute(&_cred_values(), "name")
                .unwrap();
            assert_eq!(_attr_values(), res);
        }

        #[test]
        fn get_credential_values_for_attribute_works_for_requested_attr_different_case() {
            let ps = ProverService::new();

            let res = ps
                .get_credential_values_for_attribute(&_cred_values(), "NAme")
                .unwrap();
            assert_eq!(_attr_values(), res);
        }

        #[test]
        fn get_credential_values_for_attribute_works_for_requested_attr_contains_spaces() {
            let ps = ProverService::new();

            let res = ps
                .get_credential_values_for_attribute(&_cred_values(), "   na me  ")
                .unwrap();
            assert_eq!(_attr_values(), res);
        }

        #[test]
        fn get_credential_values_for_attribute_works_for_cred_values_different_case() {
            let ps = ProverService::new();

            let cred_values = hashmap!("NAME".to_string() => _attr_values());

            let res = ps
                .get_credential_values_for_attribute(&cred_values, "name")
                .unwrap();
            assert_eq!(_attr_values(), res);
        }

        #[test]
        fn get_credential_values_for_attribute_works_for_cred_values_contains_spaces() {
            let ps = ProverService::new();

            let cred_values = hashmap!("    name    ".to_string() => _attr_values());

            let res = ps
                .get_credential_values_for_attribute(&cred_values, "name")
                .unwrap();
            assert_eq!(_attr_values(), res);
        }

        #[test]
        fn get_credential_values_for_attribute_works_for_cred_values_and_requested_attr_contains_spaces(
        ) {
            let ps = ProverService::new();

            let cred_values = hashmap!("    name    ".to_string() => _attr_values());

            let res = ps
                .get_credential_values_for_attribute(&cred_values, "            name            ")
                .unwrap();
            assert_eq!(_attr_values(), res);
        }
    }

    mod extend_operator {
        use super::*;

        const QUALIFIABLE_TAG: &str = "issuer_did";
        const NOT_QUALIFIABLE_TAG: &str = "name";
        const VALUE: &str = "1";

        #[test]
        fn extend_operator_works_for_qualifiable_tag() {
            let ps = ProverService::new();

            let query = Query::Eq(QUALIFIABLE_TAG.to_string(), VALUE.to_string());
            let query = ps._double_restrictions(query).unwrap();

            let expected_query = Query::Or(vec![
                Query::Eq(QUALIFIABLE_TAG.to_string(), VALUE.to_string()),
                Query::Eq(
                    Credential::add_extra_tag_suffix(QUALIFIABLE_TAG),
                    VALUE.to_string(),
                ),
            ]);

            assert_eq!(expected_query, query);
        }

        #[test]
        fn extend_operator_works_for_not_qualifiable_tag() {
            let ps = ProverService::new();

            let query = Query::Eq(NOT_QUALIFIABLE_TAG.to_string(), VALUE.to_string());
            let query = ps._double_restrictions(query).unwrap();

            let expected_query = Query::Eq(NOT_QUALIFIABLE_TAG.to_string(), VALUE.to_string());

            assert_eq!(expected_query, query);
        }

        #[test]
        fn extend_operator_works_for_qualifiable_tag_for_combination() {
            let ps = ProverService::new();

            let query = Query::And(vec![
                Query::Eq(QUALIFIABLE_TAG.to_string(), VALUE.to_string()),
                Query::Eq(NOT_QUALIFIABLE_TAG.to_string(), VALUE.to_string()),
            ]);
            let query = ps._double_restrictions(query).unwrap();

            let expected_query = Query::And(vec![
                Query::Or(vec![
                    Query::Eq(QUALIFIABLE_TAG.to_string(), VALUE.to_string()),
                    Query::Eq(
                        Credential::add_extra_tag_suffix(QUALIFIABLE_TAG),
                        VALUE.to_string(),
                    ),
                ]),
                Query::Eq(NOT_QUALIFIABLE_TAG.to_string(), VALUE.to_string()),
            ]);

            assert_eq!(expected_query, query);
        }
    }

    mod extend_proof_request_restrictions {
        use super::*;

        const ATTR_NAME: &str = "name";
        const ATTR_NAME_2: &str = "name_2";
        const ATTR_REFERENT: &str = "attr_1";

        fn _value(json: &str) -> serde_json::Value {
            serde_json::from_str::<serde_json::Value>(json).unwrap()
        }

        #[test]
        fn build_query_works() {
            let ps = ProverService::new();

            let query = ps
                .process_proof_request_restrictions(
                    &ProofRequestsVersion::V2,
                    &Some(ATTR_NAME.to_string()),
                    &None,
                    ATTR_REFERENT,
                    &None,
                    &None,
                )
                .unwrap();

            let expected_query = Query::And(vec![Query::Eq(
                "attr::name::marker".to_string(),
                ATTRIBUTE_EXISTENCE_MARKER.to_string(),
            )]);

            assert_eq!(expected_query, query);
        }

        #[test]
        fn build_query_works_for_name() {
            let ps = ProverService::new();

            let query = ps
                .process_proof_request_restrictions(
                    &ProofRequestsVersion::V2,
                    &None,
                    &Some(vec![ATTR_NAME.to_string(), ATTR_NAME_2.to_string()]),
                    ATTR_REFERENT,
                    &None,
                    &None,
                )
                .unwrap();

            let expected_query = Query::And(vec![
                Query::Eq(
                    "attr::name::marker".to_string(),
                    ATTRIBUTE_EXISTENCE_MARKER.to_string(),
                ),
                Query::Eq(
                    "attr::name_2::marker".to_string(),
                    ATTRIBUTE_EXISTENCE_MARKER.to_string(),
                ),
            ]);

            assert_eq!(expected_query, query);
        }

        #[test]
        fn build_query_works_for_restriction() {
            let ps = ProverService::new();

            let restriction = Query::And(vec![
                Query::Eq("schema_id".to_string(), SCHEMA_ID.to_string()),
                Query::Eq("cred_def_id".to_string(), CRED_DEF_ID.to_string()),
            ]);

            let query = ps
                .process_proof_request_restrictions(
                    &ProofRequestsVersion::V2,
                    &Some(ATTR_NAME.to_string()),
                    &None,
                    ATTR_REFERENT,
                    &Some(restriction),
                    &None,
                )
                .unwrap();

            let expected_query = Query::And(vec![
                Query::And(vec![
                    Query::Eq("schema_id".to_string(), SCHEMA_ID.to_string()),
                    Query::Eq("cred_def_id".to_string(), CRED_DEF_ID.to_string()),
                ]),
                Query::Eq(
                    "attr::name::marker".to_string(),
                    ATTRIBUTE_EXISTENCE_MARKER.to_string(),
                ),
            ]);

            assert_eq!(expected_query, query);
        }

        #[test]
        fn build_query_works_for_extra_query() {
            let ps = ProverService::new();

            let extra_query: ProofRequestExtraQuery = hashmap!(
                ATTR_REFERENT.to_string() => Query::Eq("name".to_string(), "Alex".to_string())
            );

            let query = ps
                .process_proof_request_restrictions(
                    &ProofRequestsVersion::V2,
                    &Some(ATTR_NAME.to_string()),
                    &None,
                    ATTR_REFERENT,
                    &None,
                    &Some(&extra_query),
                )
                .unwrap();

            let expected_query = Query::And(vec![
                Query::Eq("name".to_string(), "Alex".to_string()),
                Query::Eq(
                    "attr::name::marker".to_string(),
                    ATTRIBUTE_EXISTENCE_MARKER.to_string(),
                ),
            ]);

            assert_eq!(expected_query, query);
        }

        #[test]
        fn build_query_works_for_mix_restriction_and_extra_query() {
            let ps = ProverService::new();

            let restriction = Query::And(vec![
                Query::Eq("schema_id".to_string(), SCHEMA_ID.to_string()),
                Query::Eq("cred_def_id".to_string(), CRED_DEF_ID.to_string()),
            ]);

            let extra_query: ProofRequestExtraQuery = hashmap!(
                ATTR_REFERENT.to_string() => Query::Eq("name".to_string(), "Alex".to_string())
            );

            let query = ps
                .process_proof_request_restrictions(
                    &ProofRequestsVersion::V2,
                    &Some(ATTR_NAME.to_string()),
                    &None,
                    ATTR_REFERENT,
                    &Some(restriction),
                    &Some(&extra_query),
                )
                .unwrap();

            let expected_query = Query::And(vec![
                Query::And(vec![
                    Query::Eq("schema_id".to_string(), SCHEMA_ID.to_string()),
                    Query::Eq("cred_def_id".to_string(), CRED_DEF_ID.to_string()),
                ]),
                Query::Eq("name".to_string(), "Alex".to_string()),
                Query::Eq(
                    "attr::name::marker".to_string(),
                    ATTRIBUTE_EXISTENCE_MARKER.to_string(),
                ),
            ]);

            assert_eq!(expected_query, query);
        }

        #[test]
        fn build_query_works_for_extra_query_with_other_referent() {
            let ps = ProverService::new();

            let extra_query: ProofRequestExtraQuery = hashmap!(
                "other_attr_referent".to_string() => Query::Eq("name".to_string(), "Alex".to_string())
            );

            let query = ps
                .process_proof_request_restrictions(
                    &ProofRequestsVersion::V2,
                    &Some(ATTR_NAME.to_string()),
                    &None,
                    ATTR_REFERENT,
                    &None,
                    &Some(&extra_query),
                )
                .unwrap();

            let expected_query = Query::And(vec![Query::Eq(
                "attr::name::marker".to_string(),
                ATTRIBUTE_EXISTENCE_MARKER.to_string(),
            )]);

            assert_eq!(expected_query, query);
        }

        #[test]
        fn build_query_works_for_restriction_and_extra_query_contain_or_operator() {
            let ps = ProverService::new();

            let restriction = Query::Or(vec![
                Query::Eq("schema_id".to_string(), SCHEMA_ID.to_string()),
                Query::Eq("schema_id".to_string(), "schema_id_2".to_string()),
            ]);

            let extra_query: ProofRequestExtraQuery = hashmap!(
                ATTR_REFERENT.to_string() =>
                    Query::Or(vec![
                        Query::Eq("name".to_string(), "Alex".to_string()),
                        Query::Eq("name".to_string(), "Alexander".to_string()),
                    ])
            );

            let query = ps
                .process_proof_request_restrictions(
                    &ProofRequestsVersion::V2,
                    &Some(ATTR_NAME.to_string()),
                    &None,
                    ATTR_REFERENT,
                    &Some(restriction),
                    &Some(&extra_query),
                )
                .unwrap();

            let expected_query = Query::And(vec![
                Query::Or(vec![
                    Query::Eq("schema_id".to_string(), SCHEMA_ID.to_string()),
                    Query::Eq("schema_id".to_string(), "schema_id_2".to_string()),
                ]),
                Query::Or(vec![
                    Query::Eq("name".to_string(), "Alex".to_string()),
                    Query::Eq("name".to_string(), "Alexander".to_string()),
                ]),
                Query::Eq(
                    "attr::name::marker".to_string(),
                    ATTRIBUTE_EXISTENCE_MARKER.to_string(),
                ),
            ]);

            assert_eq!(expected_query, query);
        }

        #[test]
        fn build_query_works_for_restriction_by_internal_tags() {
            let ps = ProverService::new();

            let restriction = Query::And(vec![
                Query::Eq("schema_id".to_string(), SCHEMA_ID.to_string()),
                Query::Eq(
                    "attr::firstname::value".to_string(),
                    "firstname_value".to_string(),
                ),
                Query::Eq(
                    "attr::Last Name::value".to_string(),
                    "lastname_value".to_string(),
                ),
                Query::Eq("attr::File Name::marker".to_string(), "1".to_string()),
                Query::Eq("attr::textresult::marker".to_string(), "1".to_string()),
            ]);

            let query = ps
                .process_proof_request_restrictions(
                    &ProofRequestsVersion::V2,
                    &Some(ATTR_NAME.to_string()),
                    &None,
                    ATTR_REFERENT,
                    &Some(restriction),
                    &None,
                )
                .unwrap();

            let expected_query = Query::And(vec![
                Query::And(vec![
                    Query::Eq("schema_id".to_string(), SCHEMA_ID.to_string()),
                    Query::Eq(
                        "attr::firstname::value".to_string(),
                        "firstname_value".to_string(),
                    ),
                    Query::Eq(
                        "attr::lastname::value".to_string(),
                        "lastname_value".to_string(),
                    ),
                    Query::Eq("attr::filename::marker".to_string(), "1".to_string()),
                    Query::Eq("attr::textresult::marker".to_string(), "1".to_string()),
                ]),
                Query::Eq(
                    "attr::name::marker".to_string(),
                    ATTRIBUTE_EXISTENCE_MARKER.to_string(),
                ),
            ]);

            assert_eq!(expected_query, query);
        }
    }
}
