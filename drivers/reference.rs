//! Independent scalar byte/field oracle and deterministic inputs (T-owned).
use crate::content::Action;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub type State = BTreeMap<usize, Vec<u8>>;
pub fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
pub fn bytes(n: usize, seed: u64) -> Vec<u8> {
    let mut state = seed;
    (0..n).map(|_| next(&mut state) as u8).collect()
}
pub fn shuffle<T>(values: &mut [T], seed: u64) {
    let mut state = seed;
    for i in (1..values.len()).rev() {
        let j = next(&mut state) as usize % (i + 1);
        values.swap(i, j);
    }
}
pub fn initial(width: usize, seed: u64) -> State {
    (0..16)
        .map(|id| (id, bytes(width, seed.wrapping_add(id as u64))))
        .collect()
}
pub fn multiply(mut a: u8, mut b: u8) -> u8 {
    let mut result = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            result ^= a;
        }
        a = if a & 128 != 0 {
            (a << 1) ^ 0x1d
        } else {
            a << 1
        };
        b >>= 1;
    }
    result
}
pub fn inverse(a: u8) -> u8 {
    assert_ne!(a, 0);
    let mut r = 1;
    for _ in 0..254 {
        r = multiply(r, a);
    }
    r
}
pub fn apply(values: &mut State, action: &Action, width: usize, seed: u64) {
    use Action::*;
    match action {
        Insert { id } => {
            values.insert(*id, bytes(width, seed ^ 0xabcd));
        }
        Read { id } => {
            assert!(values.contains_key(id));
        }
        Zero { id } => {
            values.insert(*id, vec![0; width]);
        }
        ZeroList { ids } => {
            for id in ids {
                values.insert(*id, vec![0; width]);
            }
        }
        ZeroRange { start, count } => {
            for id in *start..start + count {
                values.insert(id, vec![0; width]);
            }
        }
        Alpha { id } => {
            for v in values.get_mut(id).unwrap() {
                *v = multiply(*v, 2);
            }
        }
        Scale { id, scalar } => {
            for v in values.get_mut(id).unwrap() {
                *v = multiply(*v, *scalar);
            }
        }
        Xor { src, dst } => xor(values, &[*src], *dst),
        XorTwo { a, b, dst } => xor(values, &[*a, *b], *dst),
        XorThree { a, b, c, dst } => xor(values, &[*a, *b, *c], *dst),
        XorMany { sources, dst } => xor(values, sources, *dst),
        Broadcast { src, destinations } => {
            let source = values[src].clone();
            for dst in destinations {
                for (a, b) in values.get_mut(dst).unwrap().iter_mut().zip(&source) {
                    *a ^= *b;
                }
            }
        }
        MulAdd { src, scalar, dst } => {
            let source = values[src].clone();
            for (a, b) in values.get_mut(dst).unwrap().iter_mut().zip(source) {
                *a ^= multiply(b, *scalar);
            }
        }
        Copy { src, dst } | Alias { src, dst } => {
            let v = values[src].clone();
            values.insert(*dst, v);
        }
        Move { src, dst } => {
            if src != dst {
                let v = values.remove(src).unwrap();
                values.insert(*dst, v);
            }
        }
        CopyRange { src, dst, count } | MoveRange { src, dst, count } => {
            let snapshot: Vec<_> = (0..*count).map(|i| values[&(src + i)].clone()).collect();
            if matches!(action, MoveRange { .. }) {
                for i in 0..*count {
                    values.remove(&(src + i));
                }
            }
            for (i, v) in snapshot.into_iter().enumerate() {
                values.insert(dst + i, v);
            }
        }
        Remove { id } => {
            values.remove(id);
        }
        Info { .. } => {}
        SharedWrite { src, dst } => {
            let mut v = values[src].clone();
            for x in &mut v {
                *x = multiply(*x, 7);
            }
            values.insert(*dst, v);
        }
    }
}
fn xor(values: &mut State, sources: &[usize], dst: usize) {
    let operands: Vec<_> = sources.iter().map(|id| values[id].clone()).collect();
    let target = values.get_mut(&dst).unwrap();
    for operand in operands {
        for (a, b) in target.iter_mut().zip(operand) {
            *a ^= b;
        }
    }
}
pub fn compact(values: &State) -> Value {
    json!({"sha256":crate::digest(&serde_json::to_vec(values).unwrap()),"symbols":values.iter().map(|(id,v)|json!({"id":id,"length":v.len(),"prefix":&v[..v.len().min(16)]})).collect::<Vec<_>>()})
}
/// Independent augmented-system elimination; None means insufficient offered rank.
pub fn solve(mut rows: Vec<Vec<u8>>, k: usize, width: usize) -> Option<Vec<u8>> {
    assert!(rows.iter().all(|r| r.len() == k + width));
    let mut pivots = vec![None; k];
    let mut rank = 0;
    for column in 0..k {
        let Some(p) = (rank..rows.len()).find(|&i| rows[i][column] != 0) else {
            continue;
        };
        rows.swap(rank, p);
        let scale = inverse(rows[rank][column]);
        for v in &mut rows[rank][column..] {
            *v = multiply(*v, scale);
        }
        let pivot = rows[rank].clone();
        for (i, row) in rows.iter_mut().enumerate() {
            if i == rank {
                continue;
            }
            let factor = row[column];
            if factor != 0 {
                for j in column..k + width {
                    row[j] ^= multiply(factor, pivot[j]);
                }
            }
        }
        pivots[column] = Some(rank);
        rank += 1;
    }
    if rank < k {
        None
    } else {
        Some(
            pivots
                .into_iter()
                .flat_map(|p| rows[p.unwrap()][k..].to_vec())
                .collect(),
        )
    }
}
