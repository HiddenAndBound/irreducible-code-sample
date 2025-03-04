use std::error;

use binius_field::{
    BinaryField, BinaryField1b, BinaryField32b, BinaryField64b, BinaryField128b, ExtensionField,
    Field, PackedExtension, RepackedExtension, TowerField, as_packed_field::PackScalar,
};
use binius_ntt::{AdditiveNTT, Error, MultithreadedNTT, SingleThreadedNTT};
use rand::thread_rng;
use rayon::{
    iter::{
        IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator,
        IntoParallelRefMutIterator, ParallelIterator,
    },
    slice::ParallelSlice,
};
use tracing::instrument;

use crate::utils::mle::LagrangeBases;

pub const RATE: usize = 4;
pub const LOG_RATE: usize = 2;
//Struct containing the Reed-Solomon encoding of a message of packed elements. We assume the elements of the message contain packed base field elements.
#[derive(Clone, Debug, Default)]
pub struct Code<F: BinaryField> {
    pub encoding: Vec<F>,
}

impl Code<BinaryField128b> {
    #[instrument(skip_all, name = "encode", level = "debug")]
    pub fn new<F>(message: &[F], ntt: &MultithreadedNTT<BinaryField128b>) -> Code<BinaryField128b>
    where
        BinaryField128b: ExtensionField<F>,
        F: BinaryField + TowerField,
    {
        let repacked_message: Vec<BinaryField128b> = message
            .par_chunks(<BinaryField128b as ExtensionField<F>>::DEGREE)
            .map(|base_elems| BinaryField128b::from_bases(base_elems).unwrap())
            .collect();
        let mut encoding = Vec::with_capacity(repacked_message.len() * RATE);
        let mut temp;

        for i in 0..RATE as u32 {
            temp = repacked_message.clone();
            ntt.forward_transform(&mut temp, i, 0).unwrap();
            encoding.append(&mut temp);
        }
        Code { encoding }
    }

    #[instrument(skip_all, name = "encode_ext", level = "debug")]
    pub fn new_ext<F, P>(message: &[F], ntt: &MultithreadedNTT<P>) -> Code<BinaryField128b>
    where
        BinaryField128b: ExtensionField<F> + ExtensionField<P> + PackedExtension<P>,
        F: BinaryField + TowerField + ExtensionField<P>,
        P: BinaryField,
    {
        let repacked_message: Vec<BinaryField128b> = message
            .par_chunks(<BinaryField128b as ExtensionField<F>>::DEGREE)
            .map(|base_elems| BinaryField128b::from_bases(base_elems).unwrap())
            .collect();

        let mut encoding = Vec::with_capacity(repacked_message.len() * RATE);
        let mut temp;

        for i in 0..RATE as u32 {
            temp = repacked_message.clone();
            ntt.forward_transform_ext::<BinaryField128b>(&mut temp, i)
                .unwrap();
            encoding.append(&mut temp);
        }
        Code { encoding }
    }

    #[instrument(skip_all, name = "fold code", level = "debug")]
    pub fn fold_code<P>(
        &self,
        r: BinaryField128b, //folding challenge
        round: usize,
        ntt: &MultithreadedNTT<P>,
    ) -> Code<BinaryField128b>
    where
        BinaryField128b: ExtensionField<P>,
        P: BinaryField,
    {
        let mut encoding: Vec<BinaryField128b> = vec![BinaryField128b::ZERO; self.encoding.len() >> 1];

        encoding.par_iter_mut().enumerate().for_each(|(i, val)| {
            *val = fold(
                r,
                round,
                i,
                self.encoding[i << 1],
                self.encoding[(i << 1) | 1],
                ntt,
            )
        });
        Code { encoding }
    }

    pub fn idx(&self, idx: usize) -> BinaryField128b {
        self.encoding[idx]
    }
}

#[inline(always)]
pub fn fold<P>(
    r: BinaryField128b,
    round: usize,
    idx: usize,
    val0: BinaryField128b,
    val1: BinaryField128b,
    ntt: &MultithreadedNTT<P>,
) -> BinaryField128b
where
    BinaryField128b: ExtensionField<P>,
    P: BinaryField,
{
    //twiddle for inverse ntt component of the fold i.e the twiddle for the butterfly unit
    //at the index if we were to apply the inverse ntt

    let twiddle = ntt.get_subspace_eval(round, idx);
    let (mut x0, mut x1) = (val0, val1);
    x1 += x0;
    x0 += x1 * twiddle;

    x0 + r * (x0 + x1)
}

#[test]
fn test_fold() {
    let l = 11;
    let poly: Vec<BinaryField128b> = (0..1 << l)
        .into_par_iter()
        .map(|_| BinaryField128b::random(thread_rng()))
        .collect();

    let ntt = SingleThreadedNTT::<BinaryField128b>::new(l + 2)
        .unwrap()
        .multithreaded();

    let code = Code::new(&poly, &ntt);

    let r: Vec<BinaryField128b> = (0..l)
        .into_iter()
        .map(|_| BinaryField128b::random(thread_rng()))
        .collect();

    let r_eq = LagrangeBases::gen_from_point(&r);

    let poly_eval: BinaryField128b = poly
        .par_iter()
        .zip(r_eq.vals)
        .map(|(coeff, eq_val)| *coeff * eq_val)
        .sum();

    let mut folded_code = code.fold_code(r[0], 0, &ntt);
    for round in 1..l {
        folded_code = folded_code.fold_code(r[round], round, &ntt)
    }

    println!("{:?}", poly_eval);
    println!("{:?}", folded_code)
}

#[test]
fn test_ntt() {
    let l = 11;
    let mut poly: Vec<BinaryField64b> = (0..1 << l)
        .into_par_iter()
        .map(|_| BinaryField64b::random(thread_rng()))
        .collect();

    let ntt = SingleThreadedNTT::<BinaryField32b>::new(13)
        .unwrap()
        .multithreaded();

    let res = ntt.forward_transform_ext(&mut poly, 0);
    match res {
        Ok(()) => println!("ok"),
        Err(error) => println!("{:?}", error),
    }
}
