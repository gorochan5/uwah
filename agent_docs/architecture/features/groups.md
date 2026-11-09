# Groups

The API exposes group creation/joining and participant listing and management. Chat handlers also expose supported group settings. Operations depend on WhatsApp permissions, current protocol support, and the capabilities implemented by the library.

Do not assume that a `group_metadata` table exists in every deployment: its historical migration file is empty in this checkout. See [the database guide](../../../docs/DATABASE.md), [the API reference](../../../docs/API.md), and [the gRPC reference](../../../docs/GRPC.md).
