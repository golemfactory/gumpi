use crate::session::SessionMan;

pub fn make(progname: String) {
    let home = dirs::home_dir().expect("Unable to get the home dir");
    let progdir = home.join("pub").join(progname);

    
}
