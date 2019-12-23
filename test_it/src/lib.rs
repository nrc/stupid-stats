fn foo(a: u8, b: String, c: usize) {
    println!("{}{}{}", a, b, c)
}

pub fn main_test() {
    foo(1, "hello".into(), 10);
    let x = 10_usize;
    // println!("{}", x);
}

fn main() {}
// #[cfg(test)]
// mod tests {
//     #[test]
//     fn it_works() {
//         assert_eq!(2 + 2, 4);
//     }
// }
