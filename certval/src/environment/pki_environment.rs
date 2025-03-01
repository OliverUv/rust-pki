//! PkiEnvironment aggregates a set of function pointers and trait objects that supply functionality
//! useful when building and/or validating a certification path, processing or generating a CMS
//! message, or performing other actions that benefit from certification path validation.
//!
//! The sample below illustrates preparation of a PkiEnvironment object for use in
//! building and validating certification paths.
//! ```
//! use certval::PkiEnvironment;
//! use certval::*;
//!
//! // the default PkiEnvironment uses `oid_lookup` to look up friendly names for OIDs
//! let mut pe = PkiEnvironment::default();
//!
//! // add basic hashing, signature verification and path validation capabilities
//! populate_5280_pki_environment(&mut pe);
//!
//! let mut ta_source = TaSource::default();
//! // populate the ta_store.buffers and ta_store.tas fields then index the trust anchors. see the
//! // `populate_parsed_ta_vector` usage in `Pittv3` for file-system based sample.
//! ta_source.index_tas();
//!
//! let mut cert_source = CertSource::default();
//! // populate the cert_source.buffers and cert_source.certs fields then index the certificates,
//! // i.e., populate the name and spki maps.
//!
//! // add ta_source and cert_source to provide access to trust anchors and intermediate CA certificates
//! pe.add_trust_anchor_source(&ta_source);
//! pe.add_certificate_source(&cert_source);
//!
//! // add certification path building capabilities
//! pe.add_path_builder(&cert_source);
//!
//! ```
//!
//! The aggregation of function pointers and trait objects allows for implementations of features to
//! vary. For example, one app may desire path validation without some PKIX features (like
//! certificate policy) processing and another may desire access to trust anchors via a system store
//! (via an FFI implementation) or much smaller sets of trust anchors for selected operations.
//!

use alloc::string::{String, ToString};
use alloc::{vec, vec::Vec};

use der::asn1::ObjectIdentifier;
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};
use x509_cert::crl::CertificateList;

use crate::PathValidationStatus::RevocationStatusNotDetermined;
use crate::{
    environment::pki_environment_traits::*, path_settings::*, util::crypto::*, util::error::*,
    util::pdv_utilities::oid_lookup, validate_path_rfc5280, CertificationPath,
    CertificationPathResults, PDVCertificate, PDVTrustAnchorChoice,
};

/// [`PkiEnvironment`] provides a switchboard of callback functions that allow support to vary on
/// different platforms or to allow support to be tailored for specific use cases.
pub struct PkiEnvironment<'a> {
    //--------------------------------------------------------------------------
    //Crypto interfaces
    //--------------------------------------------------------------------------
    /// List of functions that provide a message digest functionality
    calculate_hash_callbacks: Vec<CalculateHash>,

    /// List of functions that provide a signature verification functionality given a digest
    verify_signature_digest_callbacks: Vec<VerifySignatureDigest>,

    /// List of functions that provide a signature verification functionality given a message
    verify_signature_message_callbacks: Vec<VerifySignatureMessage>,

    //--------------------------------------------------------------------------
    //Certification path processing interfaces
    //--------------------------------------------------------------------------
    /// List of functions that provide certification path validation functionality
    validate_path_callbacks: Vec<ValidatePath>,

    /// List of trait objects that provide certification path building functionality
    #[cfg(feature = "std")]
    path_builders: Vec<&'a (dyn CertificationPathBuilder + Sync)>,
    #[cfg(not(feature = "std"))]
    path_builders: Vec<&'a dyn CertificationPathBuilder>,

    //--------------------------------------------------------------------------
    //Storage and retrieval interfaces
    //--------------------------------------------------------------------------
    /// List of trait objects that provide access to trust anchors
    #[cfg(feature = "std")]
    trust_anchor_sources: Vec<&'a (dyn TrustAnchorSource + Sync)>,
    #[cfg(not(feature = "std"))]
    trust_anchor_sources: Vec<&'a dyn TrustAnchorSource>,

    /// List of trait objects that provide access to certificates
    #[cfg(feature = "std")]
    certificate_sources: Vec<&'a (dyn CertificateSource + Sync)>,
    #[cfg(not(feature = "std"))]
    certificate_sources: Vec<&'a dyn CertificateSource>,

    /// List of trait objects that provide access to CRLs
    #[cfg(feature = "std")]
    crl_sources: Vec<&'a (dyn CrlSource + Sync)>,
    #[cfg(not(feature = "std"))]
    crl_sources: Vec<&'a dyn CrlSource>,

    /// List of trait objects that provide access to cached revocation status determinations
    #[cfg(feature = "std")]
    revocation_cache: Vec<&'a (dyn RevocationStatusCache + Sync)>,
    #[cfg(not(feature = "std"))]
    revocation_cache: Vec<&'a dyn RevocationStatusCache>,

    /// List of trait objects that provide access to blocklist and last modified info
    #[cfg(feature = "std")]
    check_remote: Vec<&'a (dyn CheckRemoteResource + Sync)>,
    #[cfg(not(feature = "std"))]
    check_remote: Vec<&'a dyn CheckRemoteResource>,

    //--------------------------------------------------------------------------
    //Miscellaneous interfaces
    //--------------------------------------------------------------------------
    /// List of functions that provide OID lookup capabilities
    oid_lookups: Vec<OidLookup>,
}

impl Default for PkiEnvironment<'_> {
    /// PkiEnvironment::default returns a new [`PkiEnvironment`] with empty callback vectors for each
    /// type of callback except `oid_lookups`, which features the [`oid_lookup`] function.
    fn default() -> Self {
        PkiEnvironment {
            calculate_hash_callbacks: vec![],
            verify_signature_digest_callbacks: vec![],
            verify_signature_message_callbacks: vec![],
            validate_path_callbacks: vec![],
            trust_anchor_sources: vec![],
            certificate_sources: vec![],
            path_builders: vec![],
            oid_lookups: vec![oid_lookup],
            crl_sources: vec![],
            revocation_cache: vec![],
            check_remote: vec![],
        }
    }
}

impl<'a> PkiEnvironment<'a> {
    /// PkiEnvironment::new returns a new [`PkiEnvironment`] with empty callback vectors for each type of callback
    pub fn new() -> PkiEnvironment<'a> {
        PkiEnvironment {
            calculate_hash_callbacks: vec![],
            verify_signature_digest_callbacks: vec![],
            verify_signature_message_callbacks: vec![],
            validate_path_callbacks: vec![],
            trust_anchor_sources: vec![],
            certificate_sources: vec![],
            path_builders: vec![],
            oid_lookups: vec![],
            crl_sources: vec![],
            revocation_cache: vec![],
            check_remote: vec![],
        }
    }

    /// clear_all_callbacks clears the contents of all function pointer and trait object vectors
    /// associated with an instance of [`PkiEnvironment`].
    pub fn clear_all_callbacks(&mut self) {
        self.clear_crl_sources();
        self.clear_path_builders();
        self.clear_oid_lookups();
        self.clear_revocation_cache();
        self.clear_certificate_sources();
        self.clear_calculate_hash_callbacks();
        self.clear_trust_anchor_sources();
        self.clear_validate_path_callbacks();
        self.clear_verify_signature_digest_callbacks();
        self.clear_verify_signature_message_callbacks();
        self.clear_check_remote_callbacks();
    }

    /// add_validate_path_callback adds a [`ValidatePath`] callback to the list used by validate_path.
    pub fn add_validate_path_callback(&mut self, c: ValidatePath) {
        self.validate_path_callbacks.push(c);
    }

    /// clear_validate_path_callbacks clears the list of [`ValidatePath`] callbacks used by validate_path.
    pub fn clear_validate_path_callbacks(&mut self) {
        self.validate_path_callbacks.clear();
    }

    /// validate_path iterates over validate_path_callbacks until an authoritative answer is found
    /// or all options have been exhausted
    pub fn validate_path(
        &self,
        pe: &PkiEnvironment<'_>,
        cps: &CertificationPathSettings,
        cp: &mut CertificationPath<'_>,
        cpr: &mut CertificationPathResults<'_>,
    ) -> Result<()> {
        let mut err = None;
        for f in &self.validate_path_callbacks {
            match f(pe, cps, cp, cpr) {
                Ok(r) => {
                    return Ok(r);
                }
                Err(e) => {
                    err = Some(e);
                }
            }
        }
        if let Some(e) = err {
            return Err(e);
        }
        Err(Error::Unrecognized)
    }

    /// add_calculate_hash_callback adds a [`CalculateHash`] callback to the list used by calculate_hash.
    pub fn add_calculate_hash_callback(&mut self, c: CalculateHash) {
        self.calculate_hash_callbacks.push(c);
    }

    /// clear_calculate_hash_callbacks clears the list of [`CalculateHash`] callbacks used by calculate_hash.
    pub fn clear_calculate_hash_callbacks(&mut self) {
        self.calculate_hash_callbacks.clear();
    }

    /// calculate_hash iterates over calculate_hash_callbacks until an authoritative answer is found
    /// or all options have been exhausted
    pub fn calculate_hash(
        &self,
        pe: &PkiEnvironment<'_>,
        hash_alg: &AlgorithmIdentifierOwned,
        buffer_to_hash: &[u8],
    ) -> Result<Vec<u8>> {
        for f in &self.calculate_hash_callbacks {
            let r = f(pe, hash_alg, buffer_to_hash);
            if let Ok(r) = r {
                return Ok(r);
            }
        }
        Err(Error::Unrecognized)
    }

    /// add_verify_signature_digest_callback adds a [`VerifySignatureDigest`] callback to the list used by verify_signature_digest.
    pub fn add_verify_signature_digest_callback(&mut self, c: VerifySignatureDigest) {
        self.verify_signature_digest_callbacks.push(c);
    }

    /// clear_verify_signature_digest_callbacks clears the list of [`VerifySignatureDigest`] callbacks used by verify_signature_digest.
    pub fn clear_verify_signature_digest_callbacks(&mut self) {
        self.verify_signature_digest_callbacks.clear();
    }

    /// verify_signature_digest iterates over verify_signature_digest_callbacks until an authoritative answer is found
    /// or all options have been exhausted
    pub fn verify_signature_digest(
        &self,
        pe: &PkiEnvironment<'_>,
        hash_to_verify: &[u8],                    // buffer to verify
        signature: &[u8],                         // signature
        signature_alg: &AlgorithmIdentifierOwned, // signature algorithm
        spki: &SubjectPublicKeyInfoOwned,         // public key
    ) -> Result<()> {
        for f in &self.verify_signature_digest_callbacks {
            let r = f(pe, hash_to_verify, signature, signature_alg, spki);
            if let Ok(r) = r {
                return Ok(r);
            }
        }
        Err(Error::Unrecognized)
    }

    /// add_verify_signature_message_callback adds a [`VerifySignatureMessage`] callback to the list used by verify_signature_message.
    pub fn add_verify_signature_message_callback(&mut self, c: VerifySignatureMessage) {
        self.verify_signature_message_callbacks.push(c);
    }

    /// clear_verify_signature_message_callbacks clears the list of [`VerifySignatureMessage`] callbacks used by verify_signature_message.
    pub fn clear_verify_signature_message_callbacks(&mut self) {
        self.verify_signature_message_callbacks.clear();
    }

    /// verify_signature_message iterates over verify_signature_message_callbacks until an authoritative answer is found
    /// or all options have been exhausted
    pub fn verify_signature_message(
        &self,
        pe: &PkiEnvironment<'_>,
        message_to_verify: &[u8],                 // buffer to verify
        signature: &[u8],                         // signature
        signature_alg: &AlgorithmIdentifierOwned, // signature algorithm
        spki: &SubjectPublicKeyInfoOwned,         // public key
    ) -> Result<()> {
        for f in &self.verify_signature_message_callbacks {
            let r = f(pe, message_to_verify, signature, signature_alg, spki);
            if let Ok(r) = r {
                return Ok(r);
            }
        }
        Err(Error::Unrecognized)
    }

    /// add_trust_anchor_source adds a [`TrustAnchorSource`] object to the list used by get_trust_anchor.
    #[cfg(feature = "std")]
    pub fn add_trust_anchor_source(&mut self, c: &'a (dyn TrustAnchorSource + Sync)) {
        self.trust_anchor_sources.push(c);
    }

    /// add_trust_anchor_source adds a [`TrustAnchorSource`] object to the list used by get_trust_anchor.
    #[cfg(not(feature = "std"))]
    pub fn add_trust_anchor_source(&mut self, c: &'a dyn TrustAnchorSource) {
        self.trust_anchor_sources.push(c);
    }

    /// clear_trust_anchor_sources clears the list of [`TrustAnchorSource`] objects used by get_trust_anchor.
    pub fn clear_trust_anchor_sources(&mut self) {
        self.trust_anchor_sources.clear();
    }

    /// get_trust_anchor iterates over trust_anchor_sources until an authoritative answer is found
    /// or all options have been exhausted
    pub fn get_trust_anchor(&self, skid: &[u8]) -> Result<&PDVTrustAnchorChoice<'_>> {
        for f in &self.trust_anchor_sources {
            let r = f.get_trust_anchor_by_skid(skid);
            if let Ok(r) = r {
                return Ok(r);
            }
        }
        Err(Error::Unrecognized)
    }

    /// get_trust_anchor iterates over trust_anchor_sources until an authoritative answer is found
    /// or all options have been exhausted
    pub fn get_trust_anchors(&self) -> Result<Vec<&PDVTrustAnchorChoice<'_>>> {
        for f in &self.trust_anchor_sources {
            let r = f.get_trust_anchors();
            if let Ok(r) = r {
                return Ok(r);
            }
        }
        Err(Error::Unrecognized)
    }

    /// get_trust_anchor_by_hex_skid returns a reference to a trust anchor corresponding to the presented hexadecimal SKID.
    pub fn get_trust_anchor_by_hex_skid(
        &'_ self,
        hex_skid: &str,
    ) -> Result<&PDVTrustAnchorChoice<'_>> {
        for f in &self.trust_anchor_sources {
            let r = f.get_trust_anchor_by_hex_skid(hex_skid);
            if let Ok(r) = r {
                return Ok(r);
            }
        }
        Err(Error::Unrecognized)
    }

    /// get_trust_anchor_for_target takes a target certificate and returns a trust anchor that may
    /// be useful in verifying the certificate.
    pub fn get_trust_anchor_for_target(
        &'_ self,
        target: &'_ PDVCertificate<'_>,
    ) -> Result<&PDVTrustAnchorChoice<'_>> {
        for f in &self.trust_anchor_sources {
            let r = f.get_trust_anchor_for_target(target);
            if let Ok(r) = r {
                return Ok(r);
            }
        }
        Err(Error::Unrecognized)
    }

    /// is_cert_a_trust_anchor takes a target certificate indication if cert is a trust anchor.
    pub fn is_cert_a_trust_anchor(&'_ self, target: &'_ PDVCertificate<'_>) -> Result<()> {
        for f in &self.trust_anchor_sources {
            if f.is_cert_a_trust_anchor(target).is_ok() {
                return Ok(());
            }
        }
        Err(Error::NotFound)
    }

    /// is_trust_anchor takes a [`PDVTrustAnchorChoice`] indication if cert is a trust anchor.
    pub fn is_trust_anchor(&'_ self, target: &'_ PDVTrustAnchorChoice<'_>) -> Result<()> {
        for f in &self.trust_anchor_sources {
            if f.is_trust_anchor(target).is_ok() {
                return Ok(());
            }
        }
        Err(Error::NotFound)
    }

    /// add_certificate_source adds a [`CertificateSource`] object to the list.
    #[cfg(feature = "std")]
    pub fn add_certificate_source(&mut self, c: &'a (dyn CertificateSource + Sync)) {
        self.certificate_sources.push(c);
    }

    /// add_certificate_source adds a [`CertificateSource`] object to the list.
    #[cfg(not(feature = "std"))]
    pub fn add_certificate_source(&mut self, c: &'a dyn CertificateSource) {
        self.certificate_sources.push(c);
    }

    /// clear_certificate_sources clears the list of [`CertificateSource`] objects.
    pub fn clear_certificate_sources(&mut self) {
        self.certificate_sources.clear();
    }

    /// add_crl_source adds a [`CrlSource`] object to the list.
    #[cfg(feature = "std")]
    pub fn add_crl_source(&mut self, c: &'a (dyn CrlSource + Sync)) {
        self.crl_sources.push(c);
    }

    /// add_crl_source adds a [`CrlSource`] object to the list.
    #[cfg(not(feature = "std"))]
    pub fn add_crl_source(&mut self, c: &'a dyn CrlSource) {
        self.crl_sources.push(c);
    }

    /// clear_crl_sources clears the list of [`CrlSource`] objects.
    pub fn clear_crl_sources(&mut self) {
        self.crl_sources.clear();
    }

    /// Retrieves CRLs for given certificate from store
    pub fn get_crls(&self, cert: &PDVCertificate<'a>) -> Result<Vec<Vec<u8>>> {
        let mut retval = vec![];
        for f in &self.crl_sources {
            if let Ok(crls) = f.get_crls(cert) {
                for crl in crls {
                    retval.push(crl);
                }
            }
        }
        if !retval.is_empty() {
            return Ok(retval);
        }
        Err(Error::NotFound)
    }

    /// Adds a CRL to the store
    pub fn add_crl(&self, crl_buf: &[u8], crl: &CertificateList, uri: &str) -> Result<()> {
        let mut at_least_one_success = false;
        for f in &self.crl_sources {
            if f.add_crl(crl_buf, crl, uri).is_ok() {
                at_least_one_success = true;
            }
        }
        if at_least_one_success {
            return Ok(());
        }
        Err(Error::NotFound)
    }

    /// add_revocation_cache adds a [`RevocationStatusCache`] object to the list.
    #[cfg(feature = "std")]
    pub fn add_revocation_cache(&mut self, c: &'a (dyn RevocationStatusCache + Sync)) {
        self.revocation_cache.push(c);
    }

    /// add_revocation_cache adds a [`RevocationStatusCache`] object to the list.
    #[cfg(not(feature = "std"))]
    pub fn add_revocation_cache(&mut self, c: &'a dyn RevocationStatusCache) {
        self.revocation_cache.push(c);
    }

    /// clear_revocation_cache clears the list of [`CertificateSource`] objects.
    pub fn clear_revocation_cache(&mut self) {
        self.revocation_cache.clear();
    }

    /// Retrieves cached revocation status determination for given certificate from store
    pub fn get_status(
        &self,
        cert: &PDVCertificate<'a>,
        time_of_interest: u64,
    ) -> PathValidationStatus {
        for f in &self.revocation_cache {
            let status = f.get_status(cert, time_of_interest);
            if RevocationStatusNotDetermined != status {
                return status;
            }
        }
        RevocationStatusNotDetermined
    }

    /// Adds a cached revocation status determination to the store
    pub fn add_status(
        &self,
        cert: &PDVCertificate<'a>,
        next_update: u64,
        status: PathValidationStatus,
    ) {
        for f in &self.revocation_cache {
            f.add_status(cert, next_update, status);
        }
    }

    /// add_path_builder adds a [`CertificationPathBuilder`] object to the list to support path building.
    #[cfg(feature = "std")]
    pub fn add_path_builder(&mut self, c: &'a (dyn CertificationPathBuilder + Sync)) {
        self.path_builders.push(c);
    }

    /// add_path_builder adds a [`CertificationPathBuilder`] object to the list to support path building.
    #[cfg(not(feature = "std"))]
    pub fn add_path_builder(&mut self, c: &'a dyn CertificationPathBuilder) {
        self.path_builders.push(c);
    }

    /// clear_path_builders clears the list of [`CertificationPathBuilder`] objects.
    pub fn clear_path_builders(&mut self) {
        self.path_builders.clear();
    }

    /// get_paths_for_target takes a target certificate and a source for trust anchors and returns
    /// a vector of [`CertificationPath`] objects.
    pub fn get_paths_for_target<'reference>(
        &'a self,
        pe: &'a PkiEnvironment<'a>,
        target: &'a PDVCertificate<'a>,
        paths: &'reference mut Vec<CertificationPath<'a>>,
        threshold: usize,
        time_of_interest: u64,
    ) -> Result<()>
    where
        'a: 'reference,
    {
        for f in &self.path_builders {
            let r = f.get_paths_for_target(pe, target, paths, threshold, time_of_interest);
            if let Ok(r) = r {
                return Ok(r);
            }
        }
        Err(Error::Unrecognized)
    }

    /// add_oid_lookup adds a oid_lookup callback to the list used by get_trust_anchors.
    pub fn add_oid_lookup(&mut self, c: OidLookup) {
        self.oid_lookups.push(c);
    }

    /// clear_oid_lookups clears the list of oid_lookup callbacks used by oid_lookup.
    pub fn clear_oid_lookups(&mut self) {
        self.oid_lookups.clear();
    }

    /// oid_lookup takes an [`ObjectIdentifier`] and returns either a friendly name for the OID or the
    /// OID represented in dot notation.
    pub fn oid_lookup(&self, oid: &ObjectIdentifier) -> String {
        for f in &self.oid_lookups {
            let r = f(oid);
            if let Ok(r) = r {
                return r;
            }
        }
        oid.to_string()
    }

    /// add_check_remote adds a [`CheckRemoteResource`] object to the list.
    #[cfg(feature = "std")]
    pub fn add_check_remote(&mut self, c: &'a (dyn CheckRemoteResource + Sync)) {
        self.check_remote.push(c);
    }

    /// add_check_remote adds a [`CheckRemoteResource`] object to the list.
    #[cfg(not(feature = "std"))]
    pub fn add_check_remote(&mut self, c: &'a dyn CheckRemoteResource) {
        self.check_remote.push(c);
    }

    /// clear_check_remote_callbacks clears the list of [`CheckRemoteResource`] objects.
    pub fn clear_check_remote_callbacks(&mut self) {
        self.check_remote.clear();
    }

    /// get_last_modified takes a URI and returns stored last modified value or None.
    pub fn get_last_modified(&self, uri: &str) -> Option<String> {
        for f in &self.check_remote {
            let r = f.get_last_modified(uri);
            if let Some(r) = r {
                return Some(r);
            }
        }
        None
    }
    /// Save last modified value, if desired
    pub fn set_last_modified(&self, uri: &str, last_modified: &str) {
        for f in &self.check_remote {
            f.set_last_modified(uri, last_modified);
        }
    }
    /// Gets blocklist takes a URI and returns true if it is on blocklist and false otherwise
    pub fn check_blocklist(&self, uri: &str) -> bool {
        for f in &self.check_remote {
            let r = f.check_blocklist(uri);
            if r {
                return true;
            }
        }
        false
    }
    /// Save blocklist, if desired
    pub fn add_to_blocklist(&self, uri: &str) {
        for f in &self.check_remote {
            f.add_to_blocklist(uri);
        }
    }
}

/// `populate_5280_pki_environment` populates a default [`PkiEnvironment`] instance with a default set of callback
/// functions specified.
///
/// The following callbacks are added:
/// - [`validate_path_rfc5280`]
/// - [`calculate_hash_rust_crypto`]
/// - [`verify_signature_digest_rust_crypto`]
/// - [`verify_signature_message_rust_crypto`]
///
/// This function assumes that [`oid_lookup`] is either present due to [`PkiEnvironment::default`] creation
/// or that it has been deliberately removed or replaced by the caller but will add oid_lookup if
/// OID lookup support is absent.
pub fn populate_5280_pki_environment(pe: &mut PkiEnvironment<'_>) {
    pe.add_validate_path_callback(validate_path_rfc5280);
    pe.add_calculate_hash_callback(calculate_hash_rust_crypto);
    pe.add_verify_signature_digest_callback(verify_signature_digest_rust_crypto);
    pe.add_verify_signature_message_callback(verify_signature_message_rust_crypto);
    if pe.oid_lookups.is_empty() {
        pe.add_oid_lookup(oid_lookup);
    }

    #[cfg(feature = "pqc")]
    pe.add_verify_signature_message_callback(verify_signature_message_pqcrypto);
    #[cfg(feature = "pqc")]
    pe.add_verify_signature_message_callback(verify_signature_message_composite_pqcrypto);
}
