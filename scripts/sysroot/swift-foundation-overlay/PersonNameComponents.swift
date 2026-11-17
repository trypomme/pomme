// Swift's PersonNameComponents, as in Apple's Foundation (open-source
// swift-foundation doesn't have it): the parts of a person's name. Firebase
// Auth keeps one for Sign in with Apple, so its type has to exist.

public struct PersonNameComponents: Hashable, Equatable, Sendable, Codable, CustomStringConvertible {
    /// "Dr.", "Ms.", …
    public var namePrefix: String?
    public var givenName: String?
    public var middleName: String?
    public var familyName: String?
    /// "Jr.", "PhD", …
    public var nameSuffix: String?
    public var nickname: String?

    public init() {}

    public init(namePrefix: String? = nil, givenName: String? = nil, middleName: String? = nil, familyName: String? = nil, nameSuffix: String? = nil, nickname: String? = nil) {
        self.namePrefix = namePrefix
        self.givenName = givenName
        self.middleName = middleName
        self.familyName = familyName
        self.nameSuffix = nameSuffix
        self.nickname = nickname
    }

    /// The parts that are set, in order, separated by spaces.
    public var description: String {
        [namePrefix, givenName, middleName, familyName, nameSuffix].compactMap { $0 }.joined(separator: " ")
    }
}
