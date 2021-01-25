/*
func subTestPacketHandler(t *testing.T) {
    network := "udp"

    m, _ := newTestManager()

    // turn server initialization
    turnSocket, err := net.ListenPacket(network, "127.0.0.1:0")
    if err != nil {
        panic(err)
    }

    // client listener initialization
    clientListener, err := net.ListenPacket(network, "127.0.0.1:0")
    if err != nil {
        panic(err)
    }

    dataCh := make(chan []byte)
    // client listener read data
    go func() {
        buffer := make([]byte, RTP_MTU)
        for {
            n, _, err2 := clientListener.ReadFrom(buffer)
            if err2 != nil {
                return
            }

            dataCh <- buffer[:n]
        }
    }()

    a, err := m.CreateAllocation(&FiveTuple{
        SrcAddr: clientListener.LocalAddr(),
        DstAddr: turnSocket.LocalAddr(),
    }, turnSocket, 0, proto.DefaultLifetime)

    assert.Nil(t, err, "should succeed")

    peerListener1, err := net.ListenPacket(network, "127.0.0.1:0")
    if err != nil {
        panic(err)
    }

    peerListener2, err := net.ListenPacket(network, "127.0.0.1:0")
    if err != nil {
        panic(err)
    }

    // add permission with peer1 address
    a.AddPermission(NewPermission(peerListener1.LocalAddr(), m.log))
    // add channel with min channel number and peer2 address
    channelBind := NewChannelBind(proto.MinChannelNumber, peerListener2.LocalAddr(), m.log)
    _ = a.AddChannelBind(channelBind, proto.DefaultLifetime)

    _, port, _ := ipnet.AddrIPPort(a.relay_socket.LocalAddr())
    relayAddrWithHostStr := fmt.Sprintf("127.0.0.1:%d", port)
    relayAddrWithHost, _ := net.ResolveUDPAddr(network, relayAddrWithHostStr)

    // test for permission and data message
    targetText := "permission"
    _, _ = peerListener1.WriteTo([]byte(targetText), relayAddrWithHost)
    data := <-dataCh

    // resolve stun data message
    assert.True(t, stun.IsMessage(data), "should be stun message")

    var msg stun.Message
    err = stun.Decode(data, &msg)
    assert.Nil(t, err, "decode data to stun message failed")

    var msgData proto.Data
    err = msgData.GetFrom(&msg)
    assert.Nil(t, err, "get data from stun message failed")
    assert.Equal(t, targetText, string(msgData), "get message doesn't equal the target text")

    // test for channel bind and channel data
    targetText2 := "channel bind"
    _, _ = peerListener2.WriteTo([]byte(targetText2), relayAddrWithHost)
    data = <-dataCh

    // resolve channel data
    assert.True(t, proto.IsChannelData(data), "should be channel data")

    channelData := proto.ChannelData{
        Raw: data,
    }
    err = channelData.Decode()
    assert.Nil(t, err, fmt.Sprintf("channel data decode with error: %v", err))
    assert.Equal(t, channelBind.Number, channelData.Number, "get channel data's number is invalid")
    assert.Equal(t, targetText2, string(channelData.Data), "get data doesn't equal the target text.")

    // listeners close
    _ = m.Close()
    _ = clientListener.Close()
    _ = peerListener1.Close()
    _ = peerListener2.Close()
}
 */
