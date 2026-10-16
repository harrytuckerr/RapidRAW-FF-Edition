// RapidRAW regression coverage for the upstream VariantStrIter soundness fix.
// Run this target with --release: the original issue is optimization-sensitive.
// Copyright (c) 2026 RapidRAW contributors. Licensed under MIT (../../../src-tauri/vendor/glib/LICENSE).

use glib::{prelude::*, Variant};

#[test]
fn borrowed_strings_survive_forward_and_reverse_iteration() {
    let values = ["", "plain ASCII", "été", "日本語", "🦀"];
    let variant = std::hint::black_box(Variant::array_from_iter::<String>(
        values.iter().map(|value| value.to_variant()),
    ));

    let forward: Vec<&str> = variant.array_iter_str().unwrap().collect();
    assert_eq!(forward, ["", "plain ASCII", "été", "日本語", "🦀"]);
    let reverse: Vec<&str> = variant.array_iter_str().unwrap().rev().collect();
    assert_eq!(reverse, ["🦀", "日本語", "été", "plain ASCII", ""]);
    // These borrowed values remain usable after their iterators have been dropped.
    assert_eq!(forward[2], "été");
    assert_eq!(reverse[1], "日本語");
}

// This test deliberately covers nth(0), including its overridden exhausted path.
#[allow(clippy::iter_nth_zero)]
#[test]
fn mixed_nth_and_double_ended_iteration_preserve_length_and_exhaustion() {
    let values = ["head", "skipped", "été", "", "tail", "日本語", "🦀"];
    let variant = std::hint::black_box(Variant::array_from_iter::<String>(
        values.iter().map(|value| value.to_variant()),
    ));
    let mut iter = variant.array_iter_str().unwrap();

    assert_eq!(iter.len(), 7);
    assert_eq!(iter.size_hint(), (7, Some(7)));
    assert_eq!(iter.next(), Some("head"));
    assert_eq!(iter.nth(1), Some("été"));
    assert_eq!(iter.len(), 4);
    assert_eq!(iter.nth_back(1), Some("日本語"));
    assert_eq!(iter.len(), 2);
    assert_eq!(iter.next(), Some(""));
    assert_eq!(iter.next_back(), Some("tail"));
    assert_eq!(iter.len(), 0);
    assert_eq!(iter.size_hint(), (0, Some(0)));
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next_back(), None);
    // Test the overridden nth(0) exhausted entrypoint; next() is covered separately.
    assert_eq!(iter.nth(0), None);
    assert_eq!(iter.nth_back(0), None);
}

#[test]
fn empty_and_singleton_arrays_have_fused_borrowed_iteration() {
    let empty = Variant::array_from_iter::<String>(std::iter::empty::<Variant>());
    let mut iter = empty.array_iter_str().unwrap();
    assert_eq!(iter.len(), 0);
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next_back(), None);
    assert_eq!(iter.count(), 0);

    let singleton = std::hint::black_box(Variant::array_from_iter::<String>(["".to_variant()]));
    let mut iter = singleton.array_iter_str().unwrap();
    assert_eq!(iter.len(), 1);
    assert_eq!(iter.next_back(), Some(""));
    assert_eq!(iter.len(), 0);
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next_back(), None);
    assert_eq!(singleton.array_iter_str().unwrap().last(), Some(""));
}

#[test]
fn overflowing_skips_exhaust_without_reading_a_child() {
    let variant = Variant::array_from_iter::<String>(["not selected".to_variant()]);
    let mut forward = variant.array_iter_str().unwrap();
    assert_eq!(forward.nth(usize::MAX), None);
    assert_eq!(forward.len(), 0);
    assert_eq!(forward.next_back(), None);

    let mut reverse = variant.array_iter_str().unwrap();
    assert_eq!(reverse.nth_back(usize::MAX), None);
    assert_eq!(reverse.len(), 0);
    assert_eq!(reverse.next(), None);
}
