import { Injectable } from "@angular/core";
import { ElectronService } from "../core/services";

/**
 * File Help Service
 */
@Injectable({
  providedIn: "root",
})
export class FileHelpService {
  /**
   * Creates an instance of file help service.
   *
   * @param electron
   */
  constructor(private electron: ElectronService) {}

  /**
   * Gets all files
   *
   * @param dirPath
   * @param [arrayOfFiles]
   * @returns all files
   */
  getAllFiles(dirPath, arrayOfFiles = []): string[] {
    const files = this.electron.fs.readdirSync(dirPath);

    files.forEach((file) => {
      if (this.electron.fs.statSync(dirPath + "/" + file).isDirectory()) {
        arrayOfFiles = this.getAllFiles(dirPath + "/" + file, arrayOfFiles);
      } else {
        arrayOfFiles.push(this.electron.path.join(dirPath, "/", file));
      }
    });

    return arrayOfFiles;
  }
}
