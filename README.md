# Stargrid
*Stargrid* is a [Cosmos](https://cosmos.network/) data availability layer. *Stargrid* is designed to ease the load on full nodes by allowing multiple tenants to connect to *Stargrid* rather than the full nodes directly. Effectively, *Stargrid* is a multiplexer for blockchain events in the Cosmos ecosystem.

Other applications could build on top of *Stargrid* to respond to arbitrary blockchain transactions such as NFT sales or coin transfers, keep a private ledger, build a time series database for price charting, or track a wallet's activity in realtime. *Stargrid* could also be used to build a blockchain like [The Graph](https://thegraph.com/) but for the Cosmos ecosystem.

## Features
*Stargrid* will be built for the following features, not necessarily in order:
- [ ] WebSocket connection to a node
- [ ] WebSocket connection to a peer (low priority)
- [ ] WebSocket connection to a consumer
- [ ] WebHook push notifications (low priority)
- [ ] Transaction filter & multiplexer
