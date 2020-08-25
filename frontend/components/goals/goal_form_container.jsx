import React from 'react'
import { useDispatch, shallowEqual, useSelector } from 'react-redux'
import GoalForm from './goal_form'
import { closeModal } from "../../actions/modal_actions";
import { clearGoalErrors, createGoal, updateGoal, deleteGoal } from '../../actions/goal_actions'

export default function goal_form_container() {

  const selectedData = useSelector((state) => ({
    errors: Object.values(state.errors.goal),
    formType: state.ui.modal.formType[0],
    passedGoal: state.ui.modal.goal[0],
    accounts: state.entities.accounts
  }), shallowEqual)

  const dispatch = useDispatch();

  let processForm;
  if (selectedData.formType === 'new') {
    processForm = (goal) => dispatch(createGoal(goal));
  } else {
    processForm = goal => dispatch(updateGoal(goal));
  };

  const modalCloser = () => dispatch(closeModal());
  const goalDeleter = (goal) => dispatch(deleteGoal(goal.id)).then(() => modalCloser());
  const goalErrorsClearer = () => dispatch(clearGoalErrors());

  const props = {
    selectedData,
    processForm,
    modalCloser,
    goalErrorsClearer,
    goalDeleter
  }



  return (
    <div className="modal-form-container">
      <GoalForm props={props}/>
    </div>
  )
};
