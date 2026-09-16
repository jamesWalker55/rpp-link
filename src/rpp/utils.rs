use rpp_parser::parser::Child;

/// Treat a `Child` as a fixed-length string array `[&str; N]`
pub fn child_as_arr<'a, const N: usize>(child: &'a Child<'a>) -> Option<[&'a str; N]> {
    let Child::Line(line) = child else {
        return None;
    };
    let Ok(res) = TryInto::<[&str; N]>::try_into(line.as_slice()) else {
        return None;
    };
    Some(res)
}

/// Treat an attr slice `&[&str]` as a fixed-length string array `[&str; N]`
pub fn attr_as_arr<'a, const N: usize>(attr: &'a [&'a str]) -> Option<[&'a str; N]> {
    let Ok(res) = TryInto::<[&str; N]>::try_into(attr) else {
        return None;
    };
    Some(res)
}
