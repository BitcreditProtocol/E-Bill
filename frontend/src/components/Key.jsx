import React, {useContext} from "react";
import copyIcon from "../assests/copy.svg";
import {MainContext} from "../context/MainContext";

export default function Key({
                                payed,
                                pending,
                                privatekey,
                                confirmations,
                                peerId,
                                payee,
                            }) {
    const {copytoClip} = useContext(MainContext);
    let iconState;
    let status;
    let privateBillKey;
    if (confirmations == 0 && payed) {
        status = "Paid";
        iconState = "lessthenthree";
    } else if (confirmations == 0 && !pending) {
        iconState = "request";
        status = "Payment Requested";
    } else if (confirmations == 0 && pending) {
        iconState = "pending";
        status = "Payment Pending";
    } else if (confirmations > 0) {
        status = "Paid" + " (" + confirmations + ")";
        if (peerId == payee?.peer_id) {
            privateBillKey = privatekey;
        }
        if (confirmations < 3) {
            iconState = "lessthenthree";
        } else if (confirmations >= 3) {
            iconState = "payed";
        }
    }
    return (
        <div className="key">
            <div className={`key-icon ${iconState}`}>
                <svg
                    className="key-icon-svg"
                    viewBox="0 0 42 42"
                    fill="none"
                    xmlns="http://www.w3.org/2000/svg"
                >
                    <path
                        d="M24.0911 25.4864L17.2222 32.3554H12.6741V36.9017H8.1278V41.448H0.549988V33.8702L16.5133 17.9069C15.9743 16.4733 15.7006 14.9535 15.7056 13.4219C15.7061 10.6766 16.5842 8.00332 18.2119 5.79252C19.8395 3.58172 22.1313 1.9492 24.7526 1.13338C27.3739 0.317557 30.1874 0.36117 32.7821 1.25785C35.3769 2.15453 37.617 3.8573 39.1753 6.11748C40.7337 8.37767 41.5285 11.0769 41.4439 13.8209C41.3593 16.5649 40.3996 19.21 38.705 21.3699C37.0103 23.5297 34.6695 25.0912 32.0245 25.8263C29.3794 26.5614 26.5686 26.4315 24.0025 25.4557L24.0911 25.4847V25.4864ZM36.9258 9.62193V9.61682C36.9258 8.71771 36.6593 7.83879 36.1598 7.09118C35.6603 6.34358 34.9503 5.76086 34.1197 5.41671C33.2891 5.07255 32.375 4.98242 31.4932 5.15771C30.6113 5.33299 29.8012 5.76582 29.1654 6.40146C28.5295 7.03711 28.0963 7.84703 27.9207 8.72883C27.7451 9.61062 27.8349 10.5247 28.1787 11.3554C28.5226 12.1862 29.105 12.8964 29.8525 13.3961C30.5999 13.8959 31.4787 14.1628 32.3778 14.1632H32.3795C33.5842 14.1627 34.7396 13.6843 35.592 12.8329C36.4443 11.9815 36.924 10.8267 36.9258 9.62193Z"/>
                </svg>
            </div>
            <div className="key-text">{status}</div>
            {privateBillKey && (
                <div
                    className="key-confirmations"
                    onClick={() =>
                        copytoClip(privateBillKey, "You have copied your Private Key")
                    }
                >
                    {confirmations} confirmations <br/>
                    {privateBillKey?.slice(0, 5) +
                        "..." +
                        privateBillKey?.slice(
                            privateBillKey?.length - 5,
                            privateBillKey?.length
                        )}{" "}
                    <img src={copyIcon}/>
                </div>
            )}
        </div>
    );
}
