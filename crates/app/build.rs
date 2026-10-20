//! Unter Windows Programmsymbol und Versionsinfo in die picsim.exe einbetten.

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/picsim.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/picsim.ico");
        res.set("ProductName", "PicSim");
        res.set("FileDescription", "PicSim, Simulator für den PIC16F84");
        res.compile().expect("Windows-Ressourcen lassen sich nicht einbetten");
    }
}
