class Api::UsersController < ApplicationController

  def create  
    @user = User.new(user_params)

    if @user.save
      Account.create(debit: true, account_category: 'Cash', institution: 'None', label: 'Greenbacks', balance: 0, user_id: @user.id)
      login!(@user)
      render 'api/users/show'
    else
      render json: @user.errors.full_messages, status: 422
    end
  end

  def user_params
    params.require(:user).permit(:email, :p_num, :password)
  end
end
