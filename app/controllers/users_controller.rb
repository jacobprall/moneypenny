class UsersController < ApplicationController
  def new
    @user = User.new()
    render :new
  end

  def create
    @user = User.new(user_params)
    if @user.save
      login!(@user)

    else
      flash[:errors] = @user.errors.full_messages
      render :new
    end
  end


  def show
    @user = current_user
    render :show
  end

  def user_params
    params.require(:user).permit(:email, :fname, :lname, :password, :avatar_file_name)
  end


end
