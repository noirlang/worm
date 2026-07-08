#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("packaging/windows/amele.ico");
    res.compile().unwrap();
}

#[cfg(not(windows))]
fn main() {}
// dostum ben hayal kırıklığıyla savaşıyorum
