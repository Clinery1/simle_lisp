use rustc_hash::FxHasher;
use bitvec::prelude::*;
use std::{
    hash::{
        Hasher,
        Hash,
    },
    rc::Rc,
};
use super::Ident;


const KEY_COUNT: usize = 64;
const KEYS: [u64; KEY_COUNT] = [    // Guaranteed to be *TRULY RANDOM*
    13643905789993782733,
    17601087771595538926,
    18344616034990649216,
    13495151125080742237,
    12384671240538025987,
    9902374356920697536,
    6761367356346684047,
    496725664337115401,
    5798843831328482111,
    15318853144534900018,
    7941213492764657388,
    12312207333678824705,
    2912110320626459414,
    1655562193101338474,
    15523857473132110999,
    2247811777188529213,
    10448757491533052183,
    7393269899048874582,
    9042810100210453465,
    10961309863808608445,
    14032929472892022955,
    14069984954139570307,
    17207257292192652759,
    12033142947922456374,
    18353257925954512045,
    17536697503350869176,
    16868366252758577101,
    4808702263167413055,
    7021761796846922022,
    11715612366691438185,
    4991810714568537531,
    2018015070469253655,
    12405556821080633606,
    9400425021953592729,
    8769980759282033729,
    1100096802933210865,
    1170785975885725218,
    13663311675884348041,
    11630720961314755979,
    547825568436792862,
    6230072209833747635,
    324475304435553913,
    15602571379087402383,
    5624368604956570485,
    15709321875513788001,
    10576312896677949890,
    8200277398995784192,
    15786938493799245451,
    18446016954144766605,
    1078478317463831464,
    1571021813789630752,
    11467534034533308819,
    10921088056182788896,
    5358262770205546493,
    8598831441923469990,
    11860745896491019303,
    9779367679907950980,
    13356126895831268925,
    10687333626689943800,
    9284768021601691533,
    17363199524491006229,
    3047399258153095890,
    6608057724672634013,
    12912302892457790644,
];


/// This minimal perfect hash function creator is based on [https://arxiv.org/abs/1603.04330]
/// The internal representation of the bitmap is like so:
/// ```text
/// |-----------|--------------|------------|--------------|-----|
/// |   name    | section_size | ones_count | section_data | ... |
/// |-----------|--------------|------------|--------------|-----|
/// |   bits    |  XXXXXXXXXX  | XXXXXXXXXX |     ...      | ... |
/// |-----------|--------------|------------|--------------|-----|
/// | bit count |   10 bits    |  10 bits   | section_size |     |
/// |           |              |            |     bits     |     |
/// |-----------|--------------|------------|--------------|-----|
/// ```
/// With this internal representation, we can store the same amount of data in a smaller footprint.
/// Tests have shown ~480 bytes for 1000 keys and ~3.0 gamma. It takes max 110us to calculate the
/// function, and max of 500ns to "hash" some data. My machine is a Ryzen 7 7840HS in a Framework 16.
pub struct MinimalPerfectHasher<const MAP_BITS: usize> {
    inner: BitVec,
    keys: Vec<Ident>,
}
impl<const MAP_BITS: usize> MinimalPerfectHasher<MAP_BITS> {
    const DEFAULT_GAMMA: f64 = 5.0;
    const MAX_BITMAP_SIZE: usize = (1 << MAP_BITS) - 1;
    const BITMAP_SIZE_BITS: usize = MAP_BITS;
    

    pub fn new(set: &[Ident], gamma: Option<f64>)->Self {
        let mut keys = Vec::new();
        let gamma = gamma.unwrap_or(Self::DEFAULT_GAMMA);
        let mut out_rows: Vec<BitVec> = Vec::new();
        let mut items;
        let mut set = set.to_vec();

        assert!(gamma >= 1.0);

        let mut iters = 0;
        while set.len() > 0 {
            let len = ((set.len() as f64 * gamma) as usize).min(Self::MAX_BITMAP_SIZE);
            items = vec![Vec::new(); len];
            assert!(len <= Self::MAX_BITMAP_SIZE);

            let key = KEYS[out_rows.len()];

            for d in set.drain(..) {
                let index = index(key, d, out_rows.len(), len);
                items[index].push(d);
            }

            let mut bv = BitVec::new();
            bv.resize(len, false);

            for (i, datas) in items.iter_mut().enumerate() {
                match datas.len() {
                    0=>continue,
                    1=>{
                        bv.set(i, true);
                        keys.push(datas[0]);
                    },
                    _=>set.append(datas),
                }
            }
            out_rows.push(bv);

            iters += 1;
            if iters == KEY_COUNT {panic!("Max depth reached")}
        }

        let mut out = BitVec::new();
        for bv in out_rows {
            assert!(bv.len() <= Self::MAX_BITMAP_SIZE);
            let len = bv.len();
            let ones_count = bv.count_ones();

            let start = out.len();
            out.extend((0..Self::BITMAP_SIZE_BITS * 2).map(|_|false));
            out[start..(start + Self::BITMAP_SIZE_BITS)].store::<usize>(len);
            out[(start + Self::BITMAP_SIZE_BITS)..(start + (Self::BITMAP_SIZE_BITS * 2))].store::<usize>(ones_count);

            out.extend(bv);
        }

        return MinimalPerfectHasher {
            inner: out,
            keys,
        };
    }

    /// Assumes the value is part of the creation set
    pub fn get_index(&self, d: Ident)->Option<usize> {
        let mut start_idx = 0;
        let mut out = 0;
        let mut key_iter = KEYS.iter().enumerate();
        while start_idx < self.inner.len() {
            let (i, key) = key_iter.next()?;
            let section_size = self.inner[start_idx..(start_idx + Self::BITMAP_SIZE_BITS)]
                .load::<usize>();
            let ones_count = self.inner[(start_idx + Self::BITMAP_SIZE_BITS)..(start_idx + (Self::BITMAP_SIZE_BITS * 2))]
                .load::<usize>();
            start_idx += Self::BITMAP_SIZE_BITS * 2;

            let slice = &self.inner[start_idx..(start_idx + section_size)];
            let index = index(*key, d, i, section_size);
            if slice[index] {
                let key_index = out + slice[..index].count_ones();
                if self.keys[key_index] == d {
                    return Some(key_index);
                }

                return None;
            }

            start_idx += section_size;
            out += ones_count;
        }

        return None;
    }

    #[inline]
    pub fn key_count(&self)->usize {
        self.keys.len()
    }
}

pub struct PerfectHashMap<T> {
    hasher: Rc<MinimalPerfectHasher<8>>,
    map: Vec<Option<T>>,
}
impl<T> PerfectHashMap<T> {
    pub fn new(set: &[Ident])->Self {
        let hasher = Rc::new(MinimalPerfectHasher::new(set, None));

        Self::from_hasher(hasher)
    }

    pub fn from_hasher(hasher: Rc<MinimalPerfectHasher<8>>)->Self {
        let mut map = Vec::new();
        map.reserve_exact(hasher.key_count());
        map.extend((0..hasher.key_count()).map(|_|None));

        Self {
            hasher,
            map,
        }
    }

    #[inline]
    pub fn hasher(&self)->Rc<MinimalPerfectHasher<8>> {
        self.hasher.clone()
    }

    pub fn get(&self, i: Ident)->Option<&T> {
        let idx = self.hasher.get_index(i)?;
        self.map[idx].as_ref()
    }

    pub fn get_mut(&mut self, i: Ident)->Option<&mut T> {
        let idx = self.hasher.get_index(i)?;
        self.map[idx].as_mut()
    }

    pub fn insert(&mut self, i: Ident, data: T)->Option<T> {
        let idx = self.hasher.get_index(i)?;
        self.map[idx].replace(data)
    }

    pub fn remove(&mut self, i: Ident)->Option<T> {
        let idx = self.hasher.get_index(i)?;
        self.map[idx].take()
    }
}


fn index(key: u64, val: Ident, depth: usize, len: usize)->usize {
    let hash = hash(key, val);
    let upper = (hash >> 32) as u32;
    let mut lower = (hash & (u32::MAX as u64)) as u32;
    lower = lower.rotate_left(depth as u32);

    return (((upper as usize) << 32) | lower as usize) % len;
}

fn hash(key: u64, val: Ident)->u64 {
    let mut hasher = FxHasher::with_seed(key as usize);
    val.hash(&mut hasher);
    return hasher.finish();
}
