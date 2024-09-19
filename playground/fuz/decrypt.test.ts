// HEX文字列をUint8Arrayに変換する関数
const hexToArrayBuffer = (hex: string) => {
    return new Uint8Array(hex.match(/.{1,2}/g)!.map(byte => parseInt(byte, 16)));
}

const decryptAES = async (buffer: ArrayBuffer, keyHex: string, ivHex: string) =>  {
    // 鍵、IV、暗号文をHEX文字列からUint8Arrayに変換
    const keyBytes = hexToArrayBuffer(keyHex);
    const ivBytes = hexToArrayBuffer(ivHex);

    // AES鍵のインポート
    const cryptoKey = await crypto.subtle.importKey(
        "raw",               // 鍵のフォーマット（"raw"でバイナリデータ）
        keyBytes,            // AESキー（Uint8Array形式）
        { name: "AES-CBC" }, // AESモードの指定（AES-CBC）
        false,               // 鍵のエクスポートを許可するか
        ["decrypt"]          // 復号操作のみを許可
    );

    // AES復号の実行
    const decryptedArrayBuffer = await crypto.subtle.decrypt(
        {
            name: "AES-CBC", // AES-CBCモード
            iv: ivBytes      // IV
        },
        cryptoKey,            // インポートしたAESキー
        buffer       // 復号する暗号化されたデータ（Uint8Array形式）
    );

    return decryptedArrayBuffer
}

// 使用例
const key = '2e009856520e10917accae78097a2e13d9dd7a97d3a5ea293527ec9d0132bba3'; // 復号鍵（HEX）
const iv = 'e8c7e042d6ba9fb85c128d5ceb64b82f'; // IV（HEX）


Deno.test("decrypt an image", async () => {
    console.log(Deno.cwd());
    const imagePath = "./playground/assets/fuz-encrypted.jpeg";
    const image = await Deno.readFile(
        imagePath
    );
    const buffer = image.buffer;

    const result = await decryptAES(buffer, key, iv);

    await Deno.writeFile("./playground/output/fuz-decrypted.jpeg", new Uint8Array(result));
})

