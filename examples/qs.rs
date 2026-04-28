fn quicksort<T: Ord>(arr: &mut [T]) {
    match arr.len() {
        0 | 1 => return,
        _ => {}
    }

    // median for the best pivot
    let mid = arr.len() / 2;
    let last = arr.len() - 1;
    if arr[0] > arr[mid] { arr.swap(0, mid); }
    if arr[0] > arr[last] { arr.swap(0, last); }
    if arr[mid] > arr[last] { arr.swap(mid, last); }

    let pivot_idx = partition(arr);

    let (left, right) = arr.split_at_mut(pivot_idx);
    quicksort(left);
    quicksort(&mut right[1..]); // skip pivot
}

fn partition<T: Ord>(arr: &mut [T]) -> usize {
    let pivot = arr.len() / 2;
    arr.swap(pivot, arr.len() - 1);

    let last = arr.len() - 1;
    let mut store = 0;

    for i in 0..last {
        if arr[i] <= arr[last] {
            arr.swap(i, store);
            store += 1;
        }
    }

    arr.swap(store, last);
    store
}

fn main() {
    let mut v = vec![3, 6, 8, 10, 1, 2, 1];
    quicksort(&mut v);
    println!("{:?}", v); // [1, 1, 2, 3, 6, 8, 10]
}
