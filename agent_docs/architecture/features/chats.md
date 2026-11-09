# Chats

Chat records are scoped to a WhatsApp session and stored in the application database. The API exposes listing, lookup, and deletion alongside operations for read state, mute, pin, archive, presence, and supported group settings.

Operations that change WhatsApp state require an active session and may depend on WhatsApp permissions. Consult the [REST API](../../../docs/API.md) and [gRPC API](../../../docs/GRPC.md) references for available methods.
