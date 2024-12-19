#[cfg(test)]
pub mod test {
    use crate::service::bill_service::BillKeys;

    pub fn get_bill_keys() -> BillKeys {
        BillKeys {
            private_key_pem: TEST_PRIVATE_KEY.to_owned(),
            public_key_pem: TEST_PUB_KEY.to_owned(),
        }
    }

    pub const TEST_PUB_KEY_SECP: &str =
        "02295fb5f4eeb2f21e01eaf3a2d9a3be10f39db870d28f02146130317973a40ac0";

    pub const TEST_PRIVATE_KEY_SECP: &str =
        "d1ff7427912d3b81743d3b67ffa1e65df2156d3dab257316cbc8d0f35eeeabe9";

    pub const TEST_PUB_KEY: &str = r#"
-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAubhgUJO9PWBZK2CfqSJr
v3RlDeF3TWiXBocWmBJXzQe4F8qfbj8nTHYJ0Eh22uPVg/Meul/3WNitFMU93jTL
hnYsx5qxOTHpQ8PVh1+2WvkpIfvJYBVuvmFMtFliyPuJKrOSGJp3SP5EgXbhSI+0
BB9y/pF5E0fZbh7Nwlci1R4L+dmuW0raPxgSgQw+g3KeBc+DiFEvJJ/ZuoaukS0h
UwDwY/QdSYRDNHNNO1W4hFJJj1dqnwfs/OmK8yWOG1GjJpI4TYnn/UO6ZJkTkTbA
xWiIC5Q+ZwzlYEJMNIBTBz+KKTUr4BeJEdneznUb0yeBzcdCg5EHQlvv7plXsQju
DQIDAQAB
-----END PUBLIC KEY-----
"#;

    pub const TEST_PRIVATE_KEY: &str = r#"
-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAubhgUJO9PWBZK2CfqSJrv3RlDeF3TWiXBocWmBJXzQe4F8qf
bj8nTHYJ0Eh22uPVg/Meul/3WNitFMU93jTLhnYsx5qxOTHpQ8PVh1+2WvkpIfvJ
YBVuvmFMtFliyPuJKrOSGJp3SP5EgXbhSI+0BB9y/pF5E0fZbh7Nwlci1R4L+dmu
W0raPxgSgQw+g3KeBc+DiFEvJJ/ZuoaukS0hUwDwY/QdSYRDNHNNO1W4hFJJj1dq
nwfs/OmK8yWOG1GjJpI4TYnn/UO6ZJkTkTbAxWiIC5Q+ZwzlYEJMNIBTBz+KKTUr
4BeJEdneznUb0yeBzcdCg5EHQlvv7plXsQjuDQIDAQABAoIBABgwJr8n1rxBKavo
HDNDi+P2DVlG9apLxmuvuWYZ8Xx/Fl9m4OfTatNfBj0tyukMRlk2l1hvuj/EjJpJ
bBreJmm/R2rBv3YzBW3xegR1F0N28v/9kockk3VRJ9PPVnnVpNI+a/cvWvzTPOnd
qU6xhKEK1YfJO4sizvM0KNk4Tw2RcE08o7tcDxQY6VO94dbaZ8ZJ2V+saiAa5BqN
cVZ+uZBmriJg+MVeB2PECqAGJWJ98r8I1Tq+2aRBc1+94E8Ilfi54qp4jpTghw3y
LH/uyf3BsbY4j08gk0Y7ljoXmaVyR7BZhcSOhc3XvMAzoqRzQpu/Fexk3Db6fuXz
wxvUW6ECgYEA/YTnhDbUS7r5ze7ntNQpuZZ3F9vU/FG2c+iC/MJ4Z2wb0gUJ8dXG
8Zbx7fQE3Hs44bW50tUaTvg7UsvyLMun6/OhdDgS+HGbMhJDNOBHQ3I1QKYUulbt
ZxRqt8dJRqOi5ctp0+zsFTko0lgA9BlIqG07oXNWzUS8Cf9DsBSaAn0CgYEAu4mg
oZok/ohv+//sb0T0UzDlxRSUf5a7Q2A0+a+hyMJm5QYHc+slLLsUdUapsx8tu71B
Y29J7+yfttH4R1NTP1cOPJj5edt+qknuQ0hMZKt+CS4ItxMM/bHV2z+Oi0U/LoW8
4jo2hh2oaHdXiDlXT9Eds7RK0vTrpcw5Q95fXtECgYAow7gecFqOmtAUJvgnAX58
Ew+vTG/g6pq15Is7bWHC74VBrgG9WyyUKDtakcQ+V6n70SbCGfYTAKM5WwXj4hNs
Q06Qy3txa4MS+BDKbc3HsJOTg6ENnXCrBINsbaUAsMs+vAiWRSBpATnpKLFujqo6
OuY9vbgVZZn+2Ybex1FEWQKBgQCAOqN9u9MtwwanDR+SGVjiBR4memLrNppGgGLY
kvGRPvNyB4RTC2Z4xlY/thhUpK31n3s1TSQGDApMzBbyVhQmzBSs9IAohR9/ultS
3/10HBpqlnJZE4qfcNhkOHnz2l5QJhu3p8weOesruuY7+9EqfzbK6Cz9P4Bc9l31
fPhC8QKBgQCO5FYksuRclILpzSVIJRj68NXZaLknDwAiNqb1a2diqGMCASXC5Z/U
jS4/cHdsAfssbxRGpoM5WNU7QPa/vhCVygcmAPPBD0DLT16JGpcnuAy3Ae4ss3Ih
HnZAVCxlGQ7ooHRIxJnp09ogDo7cDIevyMn1VmIZDm9JR1TUL6pbsg==
-----END RSA PRIVATE KEY-----
    "#;
}
