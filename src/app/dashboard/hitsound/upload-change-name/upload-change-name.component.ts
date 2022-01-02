import { Component, Inject, OnInit } from '@angular/core';
import { FormControl, Validators } from '@angular/forms';
import { ShowOnDirtyErrorStateMatcher } from '@angular/material/core';
import { MatDialogRef, MAT_DIALOG_DATA } from '@angular/material/dialog';
import { UploadChangeName } from '../../../models/uploadChangeName';
import { ValidatorService } from '../../../services/validators.service';

/**
 * Upload change name
 */
@Component({
  selector: 'app-upload-change-name',
  templateUrl: './upload-change-name.component.html',
  styleUrls: ['./upload-change-name.component.scss']
})
export class UploadChangeNameComponent implements OnInit {

  /**
   * Input answer
   */
  input: FormControl;
  /**
   * Matcher of upload change name component
   */
  matcher: ShowOnDirtyErrorStateMatcher;

  /**
   * Creates an instance of upload change name component.
   *
   * @param dialogRef
   * @param data UploadChangeName
   * @param validator
   */
  constructor(
    private dialogRef: MatDialogRef<UploadChangeNameComponent>,
    @Inject(MAT_DIALOG_DATA) public data: UploadChangeName,
    private validator: ValidatorService
  ) { }

  /**
   * on init
   */
  ngOnInit(): void {
    this.matcher = new ShowOnDirtyErrorStateMatcher();
    this.input = new FormControl('', [Validators.required, this.validator.checkName(this.data.cant), this.validator.special()]);
  }

  /**
   * Submits upload change name component
   */
  submit(): void  {
    if (!this.input.invalid) {
      this.dialogRef.close(this.input.value);
    }
  }

  /**
   * Plays file
   *
   * @param event
   * @param file
   */
  playFile(event: Event, file: File): void  {
    event.preventDefault();
    const audio = new Audio(file.path);
    audio.volume = this.data.volume / 100;
    audio.load();
    audio.play();
  }
}
