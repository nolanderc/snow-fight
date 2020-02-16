
# Protocol

## Encoding

Unless specified otherwise, all binary numbers are sent in network byte order
(big endian).


## Messages

Messages are sent between the server and client using UTF-8 encoded JSON. These
JSON strings are sent as a stream of bytes, each message starts with a channel
ID and the length of the message (in bytes), followed by that number of bytes
representing the message:

```
+------------------+-----------------+-------------------------|  |------------------+
| Channel (32-bit) | length (32-bit) | data (length bytes) ... /  / data (continued) |
+------------------+-----------------+-------------------------|  |------------------+
```
> A message

The channel id and length are single 32-bit unsigned integers.


