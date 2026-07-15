package main

import (
	"context"
	"fmt"
	"io"
	"log"
	"os"

	"github.com/songgao/water"
	"github.com/tmc/go-iroh/iroh"
	"github.com/tmc/go-iroh/key"
)

const protocolID = "fcvpn/1"

func main() {
	// 1. Avataan Windowsin TAP-kortti oikeilla Windows-asetuksilla
	config := water.Config{DeviceType: water.TAP}
	config.PlatformSpecificParams = water.PlatformSpecificParams{
		ComponentID:   "tap0901", // OpenVPN:n TAP-ajurin standarditunnus
		InterfaceName: "FC-TAP",
	}

	tapDevice, err := water.New(config)
	if err != nil {
		log.Fatal("TAP-kortin avaaminen epäonnistui (muistitko ajaa Adminina ja onko FC-TAP luotu?): ", err)
	}
	defer tapDevice.Close()

	ctx := context.Background()

	// 2. Alustetaan Iroh-päätepiste (Endpoint)
	ep, err := iroh.Bind(ctx, iroh.WithALPNs(protocolID))
	if err != nil {
		log.Fatal("Iroh alustus epäonnistui: ", err)
	}
	defer ep.Shutdown(ctx)

	// Haetaan ja näytetään Node ID (oma Iroh-osoite)
	nodeID := ep.NodeID()
	fmt.Println("--------------------------------------------------")
	fmt.Println("Kopioi tämä Iroh Node ID kaverillesi:")
	fmt.Printf("%s\n", nodeID.String())
	fmt.Println("--------------------------------------------------")

	// 3. Luodaan reititin, joka kuuntelee tulevia yhteyksiä
	router, err := iroh.NewRouter(ep)
	if err != nil {
		log.Fatal("Reitittimen luonti epäonnistui: ", err)
	}
	
	router.RegisterALPN(protocolID, func(ctx context.Context, conn iroh.Conn) {
		stream, err := conn.AcceptBidirectionalStream(ctx)
		if err == nil {
			fmt.Println("\n[TUNNELI] Kaveri yhdisti! Far Cry pitäisi nyt toimia.")
			startBridging(tapDevice, stream)
		}
	})

	go func() {
		if err := router.Serve(ctx); err != nil {
			log.Println("Reitittimen virhe:", err)
		}
	}()

	// 4. Jos annoit kaverin Node ID:n argumenttina, yhdistetään siihen
	if len(os.Args) > 1 {
		kaverinNodeIDStr := os.Args[1]
		kaverinNodeID, err := key.NodeIDFromString(kaverinNodeIDStr)
		if err != nil {
			log.Fatal("Virheellinen kaverin Node ID: ", err)
		}

		fmt.Println("Yhdistetään kaveriin Iroh-verkon kautta...")
		
		// Luodaan tyhjä NodeAddr, jotta Iroh etsii osoitteen suoraan Node ID:n perusteella
		nodeAddr := iroh.NodeAddr{
			NodeID: kaverinNodeID,
		}

		conn, err := ep.Connect(ctx, nodeAddr, protocolID)
		if err != nil {
			log.Fatal("Yhteys epäonnistui: ", err)
		}

		stream, err := conn.OpenBidirectionalStream(ctx)
		if err != nil {
			log.Fatal("Kanavan avaaminen epäonnistui: ", err)
		}

		fmt.Println("\n[TUNNELI] Yhteys muodostettu kaveriin! Tunneli valmis.")
		startBridging(tapDevice, stream)
	}

	// Pidetään ohjelma käynnissä
	select {}
}

// startBridging siirtää paketit TAP-laitteen ja Iroh-streamin välillä
func startBridging(tap *water.Interface, stream iroh.Stream) {
	// Lanka A: Luetaan pelipaketit TAP-kortilta ja lähetetään kaverille
	go func() {
		buf := make([]byte, 2000)
		for {
			n, err := tap.Read(buf)
			if err == nil {
				_, _ = stream.Write(buf[:n])
			}
		}
	}()

	// Lanka B: Otetaan vastaan kaverin pelipaketit ja syötetään ne Windowsille
	bufIn := make([]byte, 2000)
	for {
		n, err := stream.Read(bufIn)
		if err == nil {
			_, _ = tap.Write(bufIn[:n])
		} else if err == io.EOF {
			fmt.Println("\n[TUNNELI] Yhteys katkesi.")
			return
		}
	}
}
