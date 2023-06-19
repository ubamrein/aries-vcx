# Copyright (c) 2023 Ubique Innovation AG <https://www.ubique.ch>
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

cargo run --features=uniffi/cli --bin uniffi-bindgen generate ./src/vcx.udl --language kotlin --out-dir ./vcx-android/vcx/src/main/java/
cargo ndk -o ./jniLibs -t arm64-v8a -t x86_64 -t x86 -t armeabi-v7a build --release

cp -r jniLibs/* vcx-android/vcx/src/main/jniLibs/

cd vcx-android/ && ./gradlew assembleRelease
