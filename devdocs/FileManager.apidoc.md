---
title: "FileManager"
source: "https://developer.apple.com/documentation/foundation/filemanager"
author:
published:
created: 2026-10-11
description: "A convenient interface to the contents of the file system, and the primary means of interacting with it."
tags:
  - "clippings"
usernote:
---
Class

# FileManager

A convenient interface to the contents of the file system, and the primary means of interacting with it.

iOS 2.0+iPadOS 2.0+Mac Catalyst 13.0+macOS 10.0+tvOS 9.0+visionOS 1.0+watchOS 2.0+

```
class FileManager
```

## [Mentioned in](/documentation/foundation/filemanager#mentions)

[

About Apple File System](/documentation/foundation/about-apple-file-system)

[

Using the file system effectively](/documentation/foundation/using-the-file-system-effectively)

[

Optimizing Your App’s Data for iCloud Backup](/documentation/foundation/optimizing-your-app-s-data-for-icloud-backup)

## [Overview](/documentation/foundation/filemanager#overview)

A file manager object lets you examine the contents of the file system and make changes to it. The [`FileManager`](/documentation/foundation/filemanager) class provides convenient access to a shared file manager object that is suitable for most types of file-related manipulations. A file manager object is typically your primary mode of interaction with the file system. You use it to locate, create, copy, and move files and directories. You also use it to get information about a file or directory or change some of its attributes.

When specifying the location of files, you can use either [`NSURL`](/documentation/foundation/nsurl) or [`NSString`](/documentation/foundation/nsstring) objects. The use of the [`NSURL`](/documentation/foundation/nsurl) class is generally preferred for specifying file-system items because URLs can convert path information to a more efficient representation internally. You can also obtain a bookmark from an [`NSURL`](/documentation/foundation/nsurl) object, which is similar to an alias and offers a more sure way of locating the file or directory later.

If you are moving, copying, linking, or removing files or directories, you can use a delegate in conjunction with a file manager object to manage those operations. The delegate’s role is to affirm the operation and to decide whether to proceed when errors occur. In macOS 10.7 and later, the delegate must conform to the [`FileManagerDelegate`](/documentation/foundation/filemanagerdelegate) protocol.

In iOS 5.0 and later and in macOS 10.7 and later, [`FileManager`](/documentation/foundation/filemanager) includes methods for managing items stored in iCloud. Files and directories tagged for cloud storage are synced to iCloud so that they can be made available to the user’s iOS devices and Macintosh computers. Changes to an item in one location are propagated to all other locations to ensure the items stay in sync.

### [Sync control](/documentation/foundation/filemanager#Sync-control)

A [package](https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/DocumentPackages/DocumentPackages.html#//apple_ref/doc/uid/10000123i-CH106-SW1) is a directory that the system presents as a single file to the person using the device. Apps with documents that contain multiple files can use packages to manage contents like media assets. In iOS 26 and macOS 26 and later, [`FileManager`](/documentation/foundation/filemanager) introduces methods for controlling how a file provider syncs these items. By pausing sync when your app opens a package and resuming when it closes, your app can prevent the file provider from changing the contents of the package in unexpected ways, which potentially leaves the document in an inconsistent state. You can also use this pause and resume API on regular “flat” files.

### [Threading considerations](/documentation/foundation/filemanager#Threading-considerations)

The methods of the shared [`FileManager`](/documentation/foundation/filemanager) object can be called from multiple threads safely. However, if you use a delegate to receive notifications about the status of move, copy, remove, and link operations, you should create a unique instance of the file manager object, assign your delegate to that object, and use that file manager to initiate your operations.

## [Topics](/documentation/foundation/filemanager#topics)

### [Creating a file manager](/documentation/foundation/filemanager#Creating-a-file-manager)

[`convenienceinit(authorization: NSWorkspace.Authorization)`](/documentation/foundation/filemanager/init\(authorization:\))

Initializes a file manager object that is authorized to perform privileged file system operations.

[``classvar`default`: FileManager``](/documentation/foundation/filemanager/default)

The shared file manager object for the process.

### [Accessing user directories](/documentation/foundation/filemanager#Accessing-user-directories)

[`varhomeDirectoryForCurrentUser: URL`](/documentation/foundation/filemanager/homedirectoryforcurrentuser)

The home directory for the current user.

[`funcNSHomeDirectory() -> String`](/documentation/foundation/nshomedirectory\(\))

Returns the path to either the user’s or application’s home directory, depending on the platform.

[`funcNSUserName() -> String`](/documentation/foundation/nsusername\(\))

Returns the logon name of the current user.

[`funcNSFullUserName() -> String`](/documentation/foundation/nsfullusername\(\))

Returns a string containing the full name of the current user.

[`funchomeDirectory(forUser: String) -> URL?`](/documentation/foundation/filemanager/homedirectory\(foruser:\))

Returns the home directory for the specified user.

[`funcNSHomeDirectoryForUser(String?) -> String?`](/documentation/foundation/nshomedirectoryforuser\(_:\))

Returns the path to a given user’s home directory.

[`vartemporaryDirectory: URL`](/documentation/foundation/filemanager/temporarydirectory)

The temporary directory for the current user.

[`funcNSTemporaryDirectory() -> String`](/documentation/foundation/nstemporarydirectory\(\))

Returns the path of the temporary directory for the current user.

### [Locating system directories](/documentation/foundation/filemanager#Locating-system-directories)

[`funcurl(for: FileManager.SearchPathDirectory, in: FileManager.SearchPathDomainMask, appropriateFor: URL?, create: Bool) throws -> URL`](/documentation/foundation/filemanager/url\(for:in:appropriatefor:create:\))

Locates and optionally creates the specified common directory in a domain.

[`funcurls(for: FileManager.SearchPathDirectory, in: FileManager.SearchPathDomainMask) -> [URL]`](/documentation/foundation/filemanager/urls\(for:in:\))

Returns an array of URLs for the specified common directory in the requested domains.

[`funcNSSearchPathForDirectoriesInDomains(FileManager.SearchPathDirectory, FileManager.SearchPathDomainMask, Bool) -> [String]`](/documentation/foundation/nssearchpathfordirectoriesindomains\(_:_:_:\))

Creates a list of directory search paths.

[`funcNSOpenStepRootDirectory() -> String`](/documentation/foundation/nsopensteprootdirectory\(\))

Returns the root directory of the user’s system.

### [Locating application group container directories](/documentation/foundation/filemanager#Locating-application-group-container-directories)

[`funccontainerURL(forSecurityApplicationGroupIdentifier: String) -> URL?`](/documentation/foundation/filemanager/containerurl\(forsecurityapplicationgroupidentifier:\))

Returns the container directory associated with the specified security application group identifier.

[`App Groups Entitlement`](/documentation/bundleresources/entitlements/com.apple.security.application-groups)

A list of identifiers specifying the groups your app belongs to.

### [Discovering directory contents](/documentation/foundation/filemanager#Discovering-directory-contents)

[`funccontentsOfDirectory(at: URL, includingPropertiesForKeys: [URLResourceKey]?, options: FileManager.DirectoryEnumerationOptions) throws -> [URL]`](/documentation/foundation/filemanager/contentsofdirectory\(at:includingpropertiesforkeys:options:\))

Performs a shallow search of the specified directory and returns URLs for the contained items.

[`funccontentsOfDirectory(atPath: String) throws -> [String]`](/documentation/foundation/filemanager/contentsofdirectory\(atpath:\))

Performs a shallow search of the specified directory and returns the paths of any contained items.

[`funcenumerator(at: URL, includingPropertiesForKeys: [URLResourceKey]?, options: FileManager.DirectoryEnumerationOptions, errorHandler: ((URL, any Error) -> Bool)?) -> FileManager.DirectoryEnumerator?`](/documentation/foundation/filemanager/enumerator\(at:includingpropertiesforkeys:options:errorhandler:\))

Returns a directory enumerator object that can be used to perform a deep enumeration of the directory at the specified URL.

[`funcenumerator(atPath: String) -> FileManager.DirectoryEnumerator?`](/documentation/foundation/filemanager/enumerator\(atpath:\))

Returns a directory enumerator object that can be used to perform a deep enumeration of the directory at the specified path.

[`classDirectoryEnumerator`](/documentation/foundation/filemanager/directoryenumerator)

An object that enumerates the contents of a directory.

[`funcmountedVolumeURLs(includingResourceValuesForKeys: [URLResourceKey]?, options: FileManager.VolumeEnumerationOptions) -> [URL]?`](/documentation/foundation/filemanager/mountedvolumeurls\(includingresourcevaluesforkeys:options:\))

Returns an array of URLs that identify the mounted volumes available on the device.

[`structVolumeEnumerationOptions`](/documentation/foundation/filemanager/volumeenumerationoptions)

Options for enumerating mounted volumes with the [`mountedVolumeURLs(includingResourceValuesForKeys:options:)`](/documentation/foundation/filemanager/mountedvolumeurls\(includingresourcevaluesforkeys:options:\)) method.

[`funcsubpathsOfDirectory(atPath: String) throws -> [String]`](/documentation/foundation/filemanager/subpathsofdirectory\(atpath:\))

Performs a deep enumeration of the specified directory and returns the paths of all of the contained subdirectories.

[`funcsubpaths(atPath: String) -> [String]?`](/documentation/foundation/filemanager/subpaths\(atpath:\))

Returns an array of strings identifying the paths for all items in the specified directory.

### [Creating and deleting items](/documentation/foundation/filemanager#Creating-and-deleting-items)

[`funccreateDirectory(at: URL, withIntermediateDirectories: Bool, attributes: [FileAttributeKey : Any]?) throws`](/documentation/foundation/filemanager/createdirectory\(at:withintermediatedirectories:attributes:\))

Creates a directory with the given attributes at the specified URL.

[`funccreateDirectory(atPath: String, withIntermediateDirectories: Bool, attributes: [FileAttributeKey : Any]?) throws`](/documentation/foundation/filemanager/createdirectory\(atpath:withintermediatedirectories:attributes:\))

Creates a directory with given attributes at the specified path.

[`funccreateFile(atPath: String, contents: Data?, attributes: [FileAttributeKey : Any]?) -> Bool`](/documentation/foundation/filemanager/createfile\(atpath:contents:attributes:\))

Creates a file with the specified content and attributes at the given location.

[`funcremoveItem(at: URL) throws`](/documentation/foundation/filemanager/removeitem\(at:\))

Removes the file or directory at the specified URL.

[`funcremoveItem(atPath: String) throws`](/documentation/foundation/filemanager/removeitem\(atpath:\))

Removes the file or directory at the specified path.

[`functrashItem(at: URL, resultingItemURL: AutoreleasingUnsafeMutablePointer<NSURL?>?) throws`](/documentation/foundation/filemanager/trashitem\(at:resultingitemurl:\))

Moves an item to the trash.

### [Replacing items](/documentation/foundation/filemanager#Replacing-items)

[`funcreplaceItemAt(URL, withItemAt: URL, backupItemName: String?, options: FileManager.ItemReplacementOptions) throws -> URL?`](/documentation/foundation/filemanager/replaceitemat\(_:withitemat:backupitemname:options:\))

Replaces the contents of the item at the specified URL in a manner that ensures no data loss occurs.

[`funcreplaceItem(at: URL, withItemAt: URL, backupItemName: String?, options: FileManager.ItemReplacementOptions, resultingItemURL: AutoreleasingUnsafeMutablePointer<NSURL?>?) throws`](/documentation/foundation/filemanager/replaceitem\(at:withitemat:backupitemname:options:resultingitemurl:\))

Replaces the contents of the item at the specified URL in a manner that ensures no data loss occurs.

[`structItemReplacementOptions`](/documentation/foundation/filemanager/itemreplacementoptions)

Options for specifying the behavior of file replacement operations.

### [Moving and copying items](/documentation/foundation/filemanager#Moving-and-copying-items)

[`funccopyItem(at: URL, to: URL) throws`](/documentation/foundation/filemanager/copyitem\(at:to:\))

Copies the file at the specified URL to a new location synchronously.

[`funccopyItem(atPath: String, toPath: String) throws`](/documentation/foundation/filemanager/copyitem\(atpath:topath:\))

Copies the item at the specified path to a new location synchronously.

[`funcmoveItem(at: URL, to: URL) throws`](/documentation/foundation/filemanager/moveitem\(at:to:\))

Moves the file or directory at the specified URL to a new location synchronously.

[`funcmoveItem(atPath: String, toPath: String) throws`](/documentation/foundation/filemanager/moveitem\(atpath:topath:\))

Moves the file or directory at the specified path to a new location synchronously.

### [Managing iCloud-based items](/documentation/foundation/filemanager#Managing-iCloud-based-items)

[`varubiquityIdentityToken: (any NSCoding & NSCopying & NSObjectProtocol)?`](/documentation/foundation/filemanager/ubiquityidentitytoken)

An opaque token that represents the current user’s iCloud Drive Documents identity.

[`funcurl(forUbiquityContainerIdentifier: String?) -> URL?`](/documentation/foundation/filemanager/url\(forubiquitycontaineridentifier:\))

Returns the URL for the iCloud container associated with the specified identifier and establishes access to that container.

[`funcisUbiquitousItem(at: URL) -> Bool`](/documentation/foundation/filemanager/isubiquitousitem\(at:\))

Returns a Boolean indicating whether the item is targeted for storage in iCloud.

[`funcsetUbiquitous(Bool, itemAt: URL, destinationURL: URL) throws`](/documentation/foundation/filemanager/setubiquitous\(_:itemat:destinationurl:\))

Indicates whether the item at the specified URL should be stored in iCloud.

[`funcstartDownloadingUbiquitousItem(at: URL) throws`](/documentation/foundation/filemanager/startdownloadingubiquitousitem\(at:\))

Starts downloading (if necessary) the specified item to the local system.

[`funcevictUbiquitousItem(at: URL) throws`](/documentation/foundation/filemanager/evictubiquitousitem\(at:\))

Removes the local copy of the specified item that’s stored in iCloud.

[`funcurl(forPublishingUbiquitousItemAt: URL, expiration: AutoreleasingUnsafeMutablePointer<NSDate?>?) throws -> URL`](/documentation/foundation/filemanager/url\(forpublishingubiquitousitemat:expiration:\))

Returns a URL that can be emailed to users to allow them to download a copy of a flat file item from iCloud.

### [Accessing file provider services](/documentation/foundation/filemanager#Accessing-file-provider-services)

[`funcgetFileProviderServicesForItem(at: URL, completionHandler: ([NSFileProviderServiceName : NSFileProviderService]?, (any Error)?) -> Void)`](/documentation/foundation/filemanager/getfileproviderservicesforitem\(at:completionhandler:\))

Returns the services provided by the File Provider extension that manages the item at the given URL.

[`classNSFileProviderService`](/documentation/foundation/nsfileproviderservice)

A service that provides a custom communication channel between your app and a File Provider extension.

[`structNSFileProviderServiceName`](/documentation/foundation/nsfileproviderservicename)

The name used to identify a File Provider service.

### [Controlling file provider synchronization](/documentation/foundation/filemanager#Controlling-file-provider-synchronization)

[`structNSFileManagerSupportedSyncControls`](/documentation/foundation/nsfilemanagersupportedsynccontrols)

An option set of the sync controls available for an item.

[`funcpauseSyncForUbiquitousItem(at: URL, completionHandler: ((any Error)?) -> Void)`](/documentation/foundation/filemanager/pausesyncforubiquitousitem\(at:completionhandler:\))

Asynchronously pauses sync of an item at the given URL.

[`funcresumeSyncForUbiquitousItem(at: URL, with: NSFileManagerResumeSyncBehavior, completionHandler: ((any Error)?) -> Void)`](/documentation/foundation/filemanager/resumesyncforubiquitousitem\(at:with:completionhandler:\))

Asynchronously resumes the sync on a paused item using the given resume behavior.

[`enumNSFileManagerResumeSyncBehavior`](/documentation/foundation/nsfilemanagerresumesyncbehavior)

The behaviors the file manager can apply to resolve conflicts when resuming a sync.

[`funcfetchLatestRemoteVersionOfItem(at: URL, completionHandler: (NSFileVersion?, (any Error)?) -> Void)`](/documentation/foundation/filemanager/fetchlatestremoteversionofitem\(at:completionhandler:\))

Asynchronously fetches the latest remote version of a given item from the server.

[`classNSFileVersion`](/documentation/foundation/nsfileversion)

A snapshot of a file at a specific point in time.

[`funcuploadLocalVersionOfUbiquitousItem(at: URL, withConflictResolutionPolicy: NSFileManagerUploadLocalVersionConflictPolicy, completionHandler: (NSFileVersion?, (any Error)?) -> Void)`](/documentation/foundation/filemanager/uploadlocalversionofubiquitousitem\(at:withconflictresolutionpolicy:completionhandler:\))

Asynchronously uploads the local version of the item using the provided conflict resolution policy.

[`enumNSFileManagerUploadLocalVersionConflictPolicy`](/documentation/foundation/nsfilemanageruploadlocalversionconflictpolicy)

The policies the file manager can apply to resolve conflicts when uploading a local version of a file.

### [Creating symbolic and hard links](/documentation/foundation/filemanager#Creating-symbolic-and-hard-links)

[`funccreateSymbolicLink(at: URL, withDestinationURL: URL) throws`](/documentation/foundation/filemanager/createsymboliclink\(at:withdestinationurl:\))

Creates a symbolic link at the specified URL that points to an item at the given URL.

[`funccreateSymbolicLink(atPath: String, withDestinationPath: String) throws`](/documentation/foundation/filemanager/createsymboliclink\(atpath:withdestinationpath:\))

Creates a symbolic link that points to the specified destination.

[`funclinkItem(at: URL, to: URL) throws`](/documentation/foundation/filemanager/linkitem\(at:to:\))

Creates a hard link between the items at the specified URLs.

[`funclinkItem(atPath: String, toPath: String) throws`](/documentation/foundation/filemanager/linkitem\(atpath:topath:\))

Creates a hard link between the items at the specified paths.

[`funcdestinationOfSymbolicLink(atPath: String) throws -> String`](/documentation/foundation/filemanager/destinationofsymboliclink\(atpath:\))

Returns the path of the item pointed to by a symbolic link.

### [Determining access to files](/documentation/foundation/filemanager#Determining-access-to-files)

[`funcfileExists(atPath: String) -> Bool`](/documentation/foundation/filemanager/fileexists\(atpath:\))

Returns a Boolean value that indicates whether a file or directory exists at a specified path.

[`funcfileExists(atPath: String, isDirectory: UnsafeMutablePointer<ObjCBool>?) -> Bool`](/documentation/foundation/filemanager/fileexists\(atpath:isdirectory:\))

Returns a Boolean value that indicates whether a file or directory exists at a specified path.

[`funcisReadableFile(atPath: String) -> Bool`](/documentation/foundation/filemanager/isreadablefile\(atpath:\))

Returns a Boolean value that indicates whether the invoking object appears able to read a specified file.

[`funcisWritableFile(atPath: String) -> Bool`](/documentation/foundation/filemanager/iswritablefile\(atpath:\))

Returns a Boolean value that indicates whether the invoking object appears able to write to a specified file.

[`funcisExecutableFile(atPath: String) -> Bool`](/documentation/foundation/filemanager/isexecutablefile\(atpath:\))

Returns a Boolean value that indicates whether the operating system appears able to execute a specified file.

[`funcisDeletableFile(atPath: String) -> Bool`](/documentation/foundation/filemanager/isdeletablefile\(atpath:\))

Returns a Boolean value that indicates whether the invoking object appears able to delete a specified file.

### [Getting and setting attributes](/documentation/foundation/filemanager#Getting-and-setting-attributes)

[`funccomponentsToDisplay(forPath: String) -> [String]?`](/documentation/foundation/filemanager/componentstodisplay\(forpath:\))

Returns an array of strings representing the user-visible components of a given path.

[`funcdisplayName(atPath: String) -> String`](/documentation/foundation/filemanager/displayname\(atpath:\))

Returns the display name of the file or directory at a specified path.

[`funcattributesOfItem(atPath: String) throws -> [FileAttributeKey : Any]`](/documentation/foundation/filemanager/attributesofitem\(atpath:\))

Returns the attributes of the item at a given path.

[`funcattributesOfFileSystem(forPath: String) throws -> [FileAttributeKey : Any]`](/documentation/foundation/filemanager/attributesoffilesystem\(forpath:\))

Returns a dictionary that describes the attributes of the mounted file system on which a given path resides.

[`funcsetAttributes([FileAttributeKey : Any], ofItemAtPath: String) throws`](/documentation/foundation/filemanager/setattributes\(_:ofitematpath:\))

Sets the attributes of the specified file or directory.

### [Getting and comparing file contents](/documentation/foundation/filemanager#Getting-and-comparing-file-contents)

[`funccontents(atPath: String) -> Data?`](/documentation/foundation/filemanager/contents\(atpath:\))

Returns the contents of the file at the specified path.

[`funccontentsEqual(atPath: String, andPath: String) -> Bool`](/documentation/foundation/filemanager/contentsequal\(atpath:andpath:\))

Returns a Boolean value that indicates whether the files or directories in specified paths have the same contents.

### [Getting the relationship between items](/documentation/foundation/filemanager#Getting-the-relationship-between-items)

[`funcgetRelationship(UnsafeMutablePointer<FileManager.URLRelationship>, ofDirectoryAt: URL, toItemAt: URL) throws`](/documentation/foundation/filemanager/getrelationship\(_:ofdirectoryat:toitemat:\))

Determines the type of relationship that exists between a directory and an item.

[`funcgetRelationship(UnsafeMutablePointer<FileManager.URLRelationship>, of: FileManager.SearchPathDirectory, in: FileManager.SearchPathDomainMask, toItemAt: URL) throws`](/documentation/foundation/filemanager/getrelationship\(_:of:in:toitemat:\))

Determines the type of relationship that exists between a system directory and the specified item.

[`enumURLRelationship`](/documentation/foundation/filemanager/urlrelationship)

Constants indicating the relationship between a directory and an item.

### [Converting file paths to strings](/documentation/foundation/filemanager#Converting-file-paths-to-strings)

[`funcfileSystemRepresentation(withPath: String) -> UnsafePointer<CChar>`](/documentation/foundation/filemanager/filesystemrepresentation\(withpath:\))

Returns a C-string representation of a given path that properly encodes Unicode strings for use by the file system.

[`funcstring(withFileSystemRepresentation: UnsafePointer<CChar>, length: Int) -> String`](/documentation/foundation/filemanager/string\(withfilesystemrepresentation:length:\))

Returns an [`NSString`](/documentation/foundation/nsstring) object whose contents are derived from the specified C-string path.

### [Managing the delegate](/documentation/foundation/filemanager#Managing-the-delegate)

[`vardelegate: (any FileManagerDelegate)?`](/documentation/foundation/filemanager/delegate)

The delegate of the file manager object.

### [Managing the current directory](/documentation/foundation/filemanager#Managing-the-current-directory)

[`funcchangeCurrentDirectoryPath(String) -> Bool`](/documentation/foundation/filemanager/changecurrentdirectorypath\(_:\))

Changes the path of the current working directory to the specified path.

[`varcurrentDirectoryPath: String`](/documentation/foundation/filemanager/currentdirectorypath)

The path to the program’s current directory.

### [Unmounting volumes](/documentation/foundation/filemanager#Unmounting-volumes)

[`funcunmountVolume(at: URL, options: FileManager.UnmountOptions, completionHandler: ((any Error)?) -> Void)`](/documentation/foundation/filemanager/unmountvolume\(at:options:completionhandler:\))

Starts the process of unmounting the specified volume.

[`structUnmountOptions`](/documentation/foundation/filemanager/unmountoptions)

Options that specify the behavior of an unmount operation.

[`letNSFileManagerUnmountDissentingProcessIdentifierErrorKey: String`](/documentation/foundation/nsfilemanagerunmountdissentingprocessidentifiererrorkey)

The process identifier of the process that prevented a volume from unmounting.

### [Working with HFS file types](/documentation/foundation/filemanager#Working-with-HFS-file-types)

[`funcNSFileTypeForHFSTypeCode(OSType) -> String!`](/documentation/foundation/nsfiletypeforhfstypecode\(_:\))

Returns a string encoding a file type code.

[`funcNSHFSTypeCodeFromFileType(String!) -> OSType`](/documentation/foundation/nshfstypecodefromfiletype\(_:\))

Returns a file type code.

[`funcNSHFSTypeOfFile(String!) -> String!`](/documentation/foundation/nshfstypeoffile\(_:\))

Returns a string encoding a file type.

### [Determining resource fork support](/documentation/foundation/filemanager#Determining-resource-fork-support)

[`varNSFoundationVersionWithFileManagerResourceForkSupport: Int32`](/documentation/foundation/nsfoundationversionwithfilemanagerresourceforksupport)

The version of the Foundation framework in which `NSFileManager` first supported resource forks.

### [Working with notifications](/documentation/foundation/filemanager#Working-with-notifications)

[`staticletNSUbiquityIdentityDidChange: NSNotification.Name`](/documentation/foundation/nsnotification/name-swift.struct/nsubiquityidentitydidchange)

Sent after the iCloud (“ubiquity”) identity has changed.

### [Working with notification messages](/documentation/foundation/filemanager#Working-with-notification-messages)

[`structUbiquityIdentityDidChangeMessage`](/documentation/foundation/filemanager/ubiquityidentitydidchangemessage)

A message a file manager sends after the iCloud (“ubiquity”) identity changes.

### [Supporting Types](/documentation/foundation/filemanager#Supporting-Types)

[`structDirectoryEnumerationOptions`](/documentation/foundation/filemanager/directoryenumerationoptions)

Options for enumerating the contents of directories.

[`enumSearchPathDirectory`](/documentation/foundation/filemanager/searchpathdirectory)

The location of significant directories.

[`structSearchPathDomainMask`](/documentation/foundation/filemanager/searchpathdomainmask)

Domain constants specifying base locations to use when you search for significant directories.

[`structFileAttributeKey`](/documentation/foundation/fileattributekey)

Keys in dictionaries used to get and set file attributes.

[`structFileAttributeType`](/documentation/foundation/fileattributetype)

Values representing a file’s type attribute.

[`structFileProtectionType`](/documentation/foundation/fileprotectiontype)

Protection level values that can be associated with a file attribute key.

[`structURLFileProtection`](/documentation/foundation/urlfileprotection)

Protection-level values for a URL resource key.

### [Deprecated Methods](/documentation/foundation/filemanager#Deprecated-Methods)

[`funcchangeFileAttributes([AnyHashable : Any], atPath: String) -> Bool`](/documentation/foundation/filemanager/changefileattributes\(_:atpath:\))

Changes the attributes of a given file or directory.

Deprecated

[`funcfileAttributes(atPath: String, traverseLink: Bool) -> [AnyHashable : Any]?`](/documentation/foundation/filemanager/fileattributes\(atpath:traverselink:\))

Returns a dictionary that describes the POSIX attributes of the file specified at a given.

Deprecated

[`funcfileSystemAttributes(atPath: String) -> [AnyHashable : Any]?`](/documentation/foundation/filemanager/filesystemattributes\(atpath:\))

Returns a dictionary that describes the attributes of the mounted file system on which a given path resides.

Deprecated

[`funcdirectoryContents(atPath: String) -> [Any]?`](/documentation/foundation/filemanager/directorycontents\(atpath:\))

Returns the directories and files (including symbolic links) contained in a given directory.

Deprecated

[`funccreateDirectory(atPath: String, attributes: [AnyHashable : Any]) -> Bool`](/documentation/foundation/filemanager/createdirectory\(atpath:attributes:\))

Creates a directory (without contents) at a given path with given attributes.

Deprecated

[`funccreateSymbolicLink(atPath: String, pathContent: String) -> Bool`](/documentation/foundation/filemanager/createsymboliclink\(atpath:pathcontent:\))

Creates a symbolic link identified by a given path that refers to a given location.

Deprecated

[`funcpathContentOfSymbolicLink(atPath: String) -> String?`](/documentation/foundation/filemanager/pathcontentofsymboliclink\(atpath:\))

Returns the path of the directory or file that a symbolic link at a given path refers to.

Deprecated

[`funcfileManager(FileManager, shouldProceedAfterError: [AnyHashable : Any]) -> Bool`](/documentation/objectivec/nsobject-swift.class/filemanager\(_:shouldproceedaftererror:\))

An `NSFileManager` object sends this message to its handler for each error it encounters when copying, moving, removing, or linking files or directories.

Deprecated

[`funcfileManager(FileManager, willProcessPath: String)`](/documentation/objectivec/nsobject-swift.class/filemanager\(_:willprocesspath:\))

An `NSFileManager` object sends this message to a handler immediately before attempting to move, copy, rename, or delete, or before attempting to link to a given path.

Deprecated

[`funcreplaceItemAtURL(originalItemURL: NSURL, withItemAtURL: NSURL, backupItemName: String?, options: FileManager.ItemReplacementOptions) throws -> NSURL?`](/documentation/foundation/filemanager/replaceitematurl\(originalitemurl:withitematurl:backupitemname:options:\))

Replaces the contents of the item at the specified URL in a manner that ensures no data loss occurs.

Deprecated

## [Relationships](/documentation/foundation/filemanager#relationships)

### [Inherits From](/documentation/foundation/filemanager#inherits-from)

- [`NSObject`](/documentation/objectivec/nsobject-swift.class)

### [Conforms To](/documentation/foundation/filemanager#conforms-to)

- [`CVarArg`](/documentation/swift/cvararg)
- [`CustomDebugStringConvertible`](/documentation/swift/customdebugstringconvertible)
- [`CustomStringConvertible`](/documentation/swift/customstringconvertible)
- [`Equatable`](/documentation/swift/equatable)
- [`Hashable`](/documentation/swift/hashable)
- [`NSObjectProtocol`](/documentation/objectivec/nsobjectprotocol)

## [See also](/documentation/foundation/filemanager#see-also)

### [File system operations](/documentation/foundation/filemanager#File-system-operations)

[

Improving performance and stability when accessing the file system](/documentation/foundation/improving-performance-and-stability-when-accessing-the-file-system)

Prevent data loss and app crashes by interacting with the file system in a coordinated, asynchronous manner and by avoiding unnecessary disk I/O.

[

Using the file system effectively](/documentation/foundation/using-the-file-system-effectively)

Gain access to benefits like automatic backup or purging by using purpose-built directories provided by the system.

[`protocolFileManagerDelegate`](/documentation/foundation/filemanagerdelegate)

The interface a file manager’s delegate uses to intervene during operations or if an error occurs.

[

About Apple File System](/documentation/foundation/about-apple-file-system)

Use high-level APIs to get the most out of Apple File System.
