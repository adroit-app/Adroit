# Logging

Every `adroit` process — the window, the hosts, the gateway and the commands
— writes what it does to its standard error. Run as services, the hosts and
the gateway have it kept wherever the service manager keeps a service's
standard error.

`ADROIT_LOG` chooses what is written, as `tracing` filter directives such as
`debug` or `adroit_host=debug,warn`. When it is not set, Adroit's own notes
on what it is doing, warnings and errors are written, and of the libraries it
uses only their warnings and errors. A value that cannot be read is reported
and the default is used.

What a command tells the person who ran it, and the line a program ends with
when it fails, are its output and are written whatever `ADROIT_LOG` says.
