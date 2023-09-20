Reliable send

1. Construct packet and include in its header:
 - Incremented local sequence number
 - Latest received remote sequence number
 - U32 with each bit indicating the the N - 1 and older received remote sequences
2. Push the send packet on the send buffer
3. Send the packet and mark the time sent
4. Resend the packet if:
 - no acknowledgment arrives resend the packet for each 0.1s pass since u sent a packet
 - you receive an acknowledgment with a sequence larger then the packet
