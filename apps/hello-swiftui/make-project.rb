# Generate HelloSwiftUI.xcodeproj: one SwiftUI app target with the Firebase
# Apple SDK from Swift Package Manager. Run with CocoaPods' Ruby, which has
# the xcodeproj gem:
#   GEM_HOME=$(brew --prefix cocoapods)/libexec ruby make-project.rb
require "xcodeproj"

FIREBASE_VERSION = "13.1.1" # as hello-expo resolves it
PRODUCTS = %w[FirebaseCore FirebaseAnalytics FirebaseAuth FirebaseCrashlytics FirebaseFirestore]

project = Xcodeproj::Project.new("HelloSwiftUI.xcodeproj")
target = project.new_target(:application, "HelloSwiftUI", :ios, "17.0")
group = project.main_group.new_group("HelloSwiftUI", "HelloSwiftUI")
sources = %w[HelloSwiftUIApp.swift FirebaseView.swift].map { |f| group.new_reference(f) }
target.add_file_references(sources)
plist = project.main_group.new_reference("GoogleService-Info.plist")
target.resources_build_phase.add_file_reference(plist)

target.build_configurations.each do |c|
  s = c.build_settings
  s["PRODUCT_BUNDLE_IDENTIFIER"] = "dev.pomme.helloSwiftUI"
  s["PRODUCT_NAME"] = "HelloSwiftUI"
  s["MARKETING_VERSION"] = "1.0"
  s["CURRENT_PROJECT_VERSION"] = "1"
  s["SWIFT_VERSION"] = "5.0"
  s["GENERATE_INFOPLIST_FILE"] = "YES"
  s["INFOPLIST_KEY_UILaunchScreen_Generation"] = "YES"
  s["INFOPLIST_KEY_UIApplicationSceneManifest_Generation"] = "YES"
  s["INFOPLIST_KEY_CFBundleDisplayName"] = "Hello SwiftUI"
  s["TARGETED_DEVICE_FAMILY"] = "1"
  s["CODE_SIGNING_ALLOWED"] = "NO"
  # Firebase has Objective-C categories, which need -ObjC to be linked.
  s["OTHER_LDFLAGS"] = ["$(inherited)", "-ObjC"]
end

package = project.new(Xcodeproj::Project::Object::XCRemoteSwiftPackageReference)
package.repositoryURL = "https://github.com/firebase/firebase-ios-sdk"
package.requirement = { "kind" => "exactVersion", "version" => FIREBASE_VERSION }
project.root_object.package_references << package
PRODUCTS.each do |name|
  dep = project.new(Xcodeproj::Project::Object::XCSwiftPackageProductDependency)
  dep.package = package
  dep.product_name = name
  target.package_product_dependencies << dep
  file = project.new(Xcodeproj::Project::Object::PBXBuildFile)
  file.product_ref = dep
  target.frameworks_build_phase.files << file
end

project.save
puts "wrote HelloSwiftUI.xcodeproj"
