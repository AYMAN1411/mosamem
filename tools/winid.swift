// Prints the CGWindowID of the first on-screen window owned by a process with the given name.
import CoreGraphics
import Foundation
let name = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "lawha"
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in list {
  if let owner = w[kCGWindowOwnerName as String] as? String, owner.lowercased() == name.lowercased(),
     let layer = w[kCGWindowLayer as String] as? Int, layer == 0,
     let id = w[kCGWindowNumber as String] as? Int {
    print(id); exit(0)
  }
}
exit(1)
