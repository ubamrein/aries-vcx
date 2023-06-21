# Copyright (c) 2023 Ubique Innovation AG <https://www.ubique.ch>
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

cargo run --features=uniffi/cli --bin uniffi-bindgen generate src/vcx.udl --language swift --out-dir swift
mv swift/*.h headers
mv swift/*.modulemap headers

GCC_PATH=$(xcodebuild -find gcc)
XCODE_AR=$(xcodebuild -find ar)

CC=$GCC_PATH cargo build --release --target aarch64-apple-ios
CC=$GCC_PATH cargo build --release --target aarch64-apple-ios-sim
export CC=$GCC_PATH
# AR=$XCODE_AR CC=$GCC_PATH cargo build --release --target x86_64-apple-ios

# lipo -create -output swift/libuniffi_vcx.a ../../target/x86_64-apple-ios/release/libuniffi_vcx.a ../../target/aarch64-apple-ios-sim/release/libuniffi_vcx.a

rm -r ./vcxFFI.xcframework

xcodebuild -create-xcframework -library ../../target/aarch64-apple-ios/release/libuniffi_vcx.a -headers ./headers -library ../../target/aarch64-apple-ios-sim/release/libuniffi_vcx.a -headers ./headers -output vcxFFI.xcframework
