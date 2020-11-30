use crate::{
  params::*,
  indcpa::*,
  symmetric::*,
  rng::randombytes,
  error::KyberError,
  verify::{verify, cmov}
};
use rand_core::*;

// Name:        crypto_kem_keypair
//
// Description: Generates public and private key
//              for CCA-secure Kyber key encapsulation mechanism
//
// Arguments:   - unsigned char *pk: pointer to output public key (an already allocated array of CRYPTO_PUBLICKEYBYTES bytes)
//              - unsigned char *sk: pointer to output private key (an already allocated array of CRYPTO_SECRETKEYBYTES bytes)
pub fn crypto_kem_keypair<R>(
  pk: &mut[u8], 
  sk: &mut[u8], 
  rng: &mut R,
  seed: Option<([u8;32], [u8;32])> 
) -> Result<(), KyberError> 
  where R: RngCore + CryptoRng
{ 
  indcpa_keypair(pk, sk, seed, rng)?;
  let end = KYBER_INDCPA_PUBLICKEYBYTES + KYBER_INDCPA_SECRETKEYBYTES;
  sk[KYBER_INDCPA_SECRETKEYBYTES..end].copy_from_slice(&pk[..KYBER_INDCPA_PUBLICKEYBYTES]);
  
  const PK_START: usize = KYBER_SECRETKEYBYTES - (2 * KYBER_SYMBYTES);
  const SK_START: usize = KYBER_SECRETKEYBYTES-KYBER_SYMBYTES;
  hash_h(&mut sk[PK_START..], pk, KYBER_PUBLICKEYBYTES);
  // If running KAT's use seed tuple, 
  match seed {
    None => randombytes(&mut sk[SK_START..],KYBER_SYMBYTES, rng),
    Some(s) => {
      sk[SK_START..].copy_from_slice(&s.1);
      Ok(())
    }
  }
}


// Name:        crypto_kem_enc
//
// Description: Generates cipher text and shared
//              secret for given public key
//
// Arguments:   - unsigned char *ct:       pointer to output cipher text (an already allocated array of CRYPTO_CIPHERTEXTBYTES bytes)
//              - unsigned char *ss:       pointer to output shared secret (an already allocated array of CRYPTO_BYTES bytes)
//              - const unsigned char *pk: pointer to input public key (an already allocated array of CRYPTO_PUBLICKEYBYTES bytes)
pub fn crypto_kem_enc<R>(
  ct: &mut[u8], 
  ss: &mut[u8], 
  pk: &[u8],
  rng: &mut R,
  seed: Option<&[u8]>
) -> Result<(), KyberError>
  where R: RngCore + CryptoRng
{
  let mut kr = [0u8; 2*KYBER_SYMBYTES];
  let mut buf = [0u8; 2*KYBER_SYMBYTES];
  let mut randbuf = [0u8; 2*KYBER_SYMBYTES];

  let res = match seed {
    // Retreive OS randombytes
    None => randombytes(&mut randbuf, KYBER_SYMBYTES, rng),
    // Deterministic randbuf for KAT's
    Some(s) => {randbuf[..KYBER_SYMBYTES].copy_from_slice(&s); Ok(())}
  };

  // Don't release system RNG output 
  hash_h(&mut buf, &randbuf, KYBER_SYMBYTES);

  // Multitarget countermeasure for coins + contributory KEM
  hash_h(&mut buf[KYBER_SYMBYTES..], pk, KYBER_PUBLICKEYBYTES);
  hash_g(&mut kr, &buf, 2*KYBER_SYMBYTES);

  // coins are in kr[KYBER_SYMBYTES..]
  indcpa_enc(ct, &buf, pk, &kr[KYBER_SYMBYTES..]);

  // overwrite coins in kr with H(c) 
  hash_h(&mut kr[KYBER_SYMBYTES..], ct, KYBER_CIPHERTEXTBYTES);

  // hash concatenation of pre-k and H(c) to k
  kdf(ss, &kr, 2*KYBER_SYMBYTES as u64);
  res
}


// Name:        crypto_kem_dec
//
// Description: Generates shared secret for given
//              cipher text and private key
//
// Arguments:   - unsigned char *ss:       pointer to output shared secret (an already allocated array of CRYPTO_BYTES bytes)
//              - const unsigned char *ct: pointer to input cipher text (an already allocated array of CRYPTO_CIPHERTEXTBYTES bytes)
//              - const unsigned char *sk: pointer to input private key (an already allocated array of CRYPTO_SECRETKEYBYTES bytes)
//
// On failure, ss will contain a pseudo-random value.
pub fn crypto_kem_dec(ss: &mut[u8], ct: &[u8], sk: &[u8]) -> Result<(), KyberError> {
  let mut buf = [0u8; 2*KYBER_SYMBYTES];
  let mut kr = [0u8; 2*KYBER_SYMBYTES];
  let mut cmp = [0u8; KYBER_CIPHERTEXTBYTES];
  let mut pk = [0u8; KYBER_INDCPA_PUBLICKEYBYTES + 2*KYBER_SYMBYTES];

  pk.copy_from_slice(&sk[KYBER_INDCPA_SECRETKEYBYTES..]);

  indcpa_dec(&mut buf, ct, sk);
  for i in 0..KYBER_SYMBYTES {
    // Save hash by storing H(pk) in sk 
    buf[KYBER_SYMBYTES+i] = sk[KYBER_SECRETKEYBYTES-2*KYBER_SYMBYTES+i];
  }
  hash_g(&mut kr, &buf, 2*KYBER_SYMBYTES);
  // coins are in kr[KYBER_SYMBYTES..] 
  indcpa_enc(&mut cmp, &buf, &pk, &kr[KYBER_SYMBYTES..]);

  let fail = verify(ct, &cmp, KYBER_CIPHERTEXTBYTES);
  // overwrite coins in kr with H(c)
  hash_h(&mut kr[KYBER_SYMBYTES..], ct, KYBER_CIPHERTEXTBYTES);
  // Overwrite pre-k with z on re-encryption failure 
  cmov(&mut kr, &sk[KYBER_SECRETKEYBYTES-KYBER_SYMBYTES..], KYBER_SYMBYTES, fail);
  // hash concatenation of pre-k and H(c) to k 
  kdf(ss, &kr, 2*KYBER_SYMBYTES as u64);

  match fail {
    0 => Ok(()),
    _ => Err(KyberError::DecodeFail)
  }
}
