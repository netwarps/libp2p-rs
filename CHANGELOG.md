## 0.3.0 release (2021.4.23)

### Changes

Eventually we don't depend on ReadEx/WriteEx/SplitEx any longer. Actually we figure out it is better to keep the classic AsyncRead/AsyncWrite, for the sake of backward compatibility. On the other hand, we still have ReadEx/WriteEx, which serve as extensions to AsyncRead/AsyncWrite, providing IO Codec support.   

Besides, there are some changes in Kad-DHT protocol implementation. To be more specific, re-provide/publish functionality is removed from Kad. We tend to believe the logics of re-provide/republish belong to the App using Kad, instead of Kad itself. As a result, the data structures of Provider/Record are simplified accordingly, and there are two gc_xxx methods are required in RecordStore trait, to perform GC for Provider and Records respectively, which is done by tracking the timestamp of Provider/Record received from network. Note that GC will be performed only for the provider/record received from network.      

## 0.2.2 release (2021.3.1)

### Changes

- Swarm:
    * Must implement ProtocolImpl trait for all protocols
        + ProtocolImpl includes two methods: handler() & start()
    * Protocols main loop(if any) are started by Swarm  
- Floodsub 
    * move .await to independent tasks
    * API: Arc<FloodsubMessage> to avoid cloning messages for multiple subscribers
- Kad API changed
    * bootstrap() allow an initial boot node list
    * unprovide() to remove provider from local store
- Other minor changes    
    
## 0.2.1 release (2021.1.26)

- libp2prs_runtime added to support both async-std and tokio 1.0
- feature sets refactored    


## 0.2.0 release (2021.1.12)

### New features and changes

- Protocols
    + **Kad-DHT**:
        * beta-value introduced to better handle iterative query termination
        * configurable timeout for iterative query
        * auto-refresh mechanism to refresh the routing table in a configurable interval
        * health check for any nodes/peers in the routing table whose aliveness is deemed to be outdated
        * event handling for peer identified and local address changed
        * outgoing sub-streams reuse
        * statistics for iterative query - success, failure or timeout
        * debugging shell commands           
    + floodsub: experimental
    + mDns: experimental
- Swarm
    + async post-processing upgrade when accepting new incoming connections**
    + dialer support, dialing multiple addresses in parallel
    + improved identify protocol
    + metric support and many bug fixes
    + notification mechanism for protocol handlers
- Tcp transport: interface address change event
- PeerStore improvement
- Prometheus exporter and Info web server
- An interactive debugging shell, integrated with Swarm and Kad
- Copyright notice updated to conform with MIT license

## 0.1.0 Initial release (2020.10.26)

### Features

- Transport: Tcp, Dns, Websocket
- Security IO: secio, plaintext, noise
- Stream Muxing: yamux, mplex
- Transport Upgrade: Multistream select, timeout, protector 
- Swarm, with Ping & Identify