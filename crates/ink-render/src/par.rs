//! Work spread over the machine's cores with std's scoped threads: the
//! renderer's bands of rows. Results come back in the order the items
//! went in, whatever order they finish in. (LS3's vector module and
//! lntrn-image have the same helper, each crate-private; this is a copy
//! rather than an API change there.)

use std::sync::Mutex;

/// `f` of every item, on as many threads as there are cores (or items).
/// One item, or one core, runs on the calling thread.
pub(crate) fn map<I: Send, T: Send>(items: Vec<I>, f: impl Fn(I) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(items.len());
    if threads <= 1 {
        return items.into_iter().map(f).collect();
    }
    let queue = Mutex::new(items.into_iter().enumerate());
    let mut done: Vec<(usize, T)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let next = queue.lock().unwrap_or_else(|e| e.into_inner()).next();
                        let Some((i, item)) = next else { break };
                        out.push((i, f(item)));
                    }
                    out
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))).collect()
    });
    done.sort_unstable_by_key(|&(i, _)| i);
    done.into_iter().map(|(_, t)| t).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn results_keep_the_items_order() {
        let squares = super::map((0..1000u64).collect(), |i| i * i);
        assert!(squares.iter().enumerate().all(|(i, &s)| s == (i * i) as u64));
        assert_eq!(super::map(Vec::<u8>::new(), |b| b), Vec::<u8>::new());
    }
}
