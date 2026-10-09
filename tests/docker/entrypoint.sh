#!/bin/sh
# TCP echo server on 7777 for port-forwarding tests.
socat TCP-LISTEN:7777,fork,reuseaddr EXEC:cat &
exec /usr/sbin/sshd -D -e
