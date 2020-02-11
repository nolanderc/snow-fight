
# Protocol

## Encoding

Unless specified otherwise, all binary numbers are sent in network byte order
(big endian).


## Messages

Messages are sent between the server and client using UTF-8 encoded JSON. These
JSON strings are sent as a stream of bytes, each message startng with the length
of the message (in bytes), followed by that number of bytes representing the
message:

```
+-----------------+-------------------------|  |------------------+
| length (32-bit) | data (length bytes) ... /  / data (continued) |
+-----------------+-------------------------|  |------------------+
```
> A message

The length is a single 32-bit unsigned integer.




