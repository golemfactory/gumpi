use clap::{crate_authors, crate_version, App, Arg};

const APP: &str = env!("CARGO_PKG_NAME");

pub fn get_parser<'a, 'b>() -> App<'a, 'b> {
    App::new(APP)
        .version(crate_version!())
        .author(crate_authors!())
        .about("Poor man's slurm")
        .arg(
            Arg::with_name("numproc")
                .short("n")
                .long("numproc")
                .required(true)
                .takes_value(true)
                .help("Total number of processes"),
        )
        .arg(
            Arg::with_name("progname")
                .short("p")
                .long("progname")
                .required(true)
                .multiple(true)
                .takes_value(true)
                .help("The URL to poll"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let matches =
            get_parser().get_matches_from(&["./executable", "-n", "15", "-p", "www.example.com"]);

        assert_eq!(matches.value_of("numproc").unwrap(), "15");
        assert_eq!(matches.value_of("progname").unwrap(), "www.example.com");
    }
}
