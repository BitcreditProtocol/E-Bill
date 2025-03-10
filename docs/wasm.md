# WASM Setup

### Building

Make sure to have at least Rust version 1.85 as well as a recent version of the toolchain installed.

You also need the `wasm-pack` tool from [here](https://rustwasm.github.io/wasm-pack/installer/).

```bash
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

#### Development

```bash
wasm-pack build --dev --target web
```

#### Release

```bash
wasm-pack build --target web
```

### Running

After building the project for WASM, you'll find the WASM artifacts in the `./pkg` folder including generated TypeScript bindings.

You can run this by serving it to the web, using any local HTTP-Server. For example, you can use [http-server](https://www.npmjs.com/package/http-server).

There are example `index.html` and `main.js` files, which provide a playground to test the created WASM artifacts.

```bash
http-server -c-1 .
```

This way, you can interact with the app at [http://localhost:8080/](http://localhost:8080/).

The database used by Surreal is IndexedDb in the WASM version, so if you clear your IndexedDb (Dev tools -> Storage), you can reset it.
Also, opening the app in another browser, or a private browser window, also starts with a blank slate.

### API

The API can be used in the following way (you can also check more examples in `main.js`):

#### API Example

```javascript
import * as wasm from '../pkg/bcr_ebill_pwa.js';

async function start() {
    let config = {
        bitcoin_network: "testnet",
        nostr_relay: "wss://bitcr-cloud-run-04-550030097098.europe-west1.run.app",
        surreal_db_connection: "indxdb://default",
        data_dir: ".",
        job_runner_initial_delay_seconds: 1,
        job_runner_check_interval_seconds: 600,
    };

    await wasm.default();
    await wasm.initialize_api(config);

    let notificationApi = wasm.Api.notification();
    let identityApi = wasm.Api.identity();
    let contactApi = wasm.Api.contact();
    let companyApi = wasm.Api.company();
    let billApi = wasm.Api.bill();
    let generalApi = wasm.Api.general();

    let identity;
    // Identity
    try {
        identity = await identityApi.detail();
        console.log("local identity:", identity);
    } catch(err) {
        console.log("No local identity found - creating..");
        await identityApi.create({
            name: "Johanna Smith",
            email: "jsmith@example.com",
            postal_address: {
                country: "AT",
                city: "Vienna",
                zip: "1020",
                address: "street 1",
            }
        });
        identity = await identityApi.detail();
    }

    await notificationApi.subscribe((evt) => {
        console.log("Received event in JS: ", evt);
    });
}

await start();
```

