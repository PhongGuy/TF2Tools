import { Component, Inject, OnInit } from "@angular/core";
import { FormControl, Validators } from "@angular/forms";
import { ShowOnDirtyErrorStateMatcher } from "@angular/material/core";
import { MatDialogRef, MAT_DIALOG_DATA } from "@angular/material/dialog";
import { QuestionAnswer } from "../../models/questionAnswer";
import { ValidatorService } from "../../services/validators.service";

/**
 * Component QuestionAnswerComponent
 */
@Component({
  selector: "app-question-answer",
  templateUrl: "./question-answer.component.html",
  styleUrls: ["./question-answer.component.scss"],
})
export class QuestionAnswerComponent implements OnInit {
  /**
   * Input answer
   */
  input: FormControl;
  /**
   * Matcher of question answer component
   */
  matcher: ShowOnDirtyErrorStateMatcher;

  /**
   * Creates an instance of question answer component.
   *
   * @param dialogRef
   * @param data `QuestionAnswer`
   * @param validator
   */
  constructor(
    private dialogRef: MatDialogRef<QuestionAnswerComponent>,
    @Inject(MAT_DIALOG_DATA) public data: QuestionAnswer,
    private validator: ValidatorService
  ) {}

  /**
   * on init
   */
  ngOnInit(): void {
    this.matcher = new ShowOnDirtyErrorStateMatcher();
    this.input = new FormControl("", [
      Validators.required,
      this.validator.checkName(this.data.cant),
      this.validator.special(),
    ]);
  }

  /**
   * Submits question answer component
   */
  submit() {
    if (!this.input.invalid) {
      this.dialogRef.close(this.input.value);
    }
  }
}
